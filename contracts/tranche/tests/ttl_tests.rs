//! #1308: the tranche contract used to never call `extend_ttl`, so its
//! instance storage entry — which holds ALL of the pool's state
//! (`TranchePool`, `InvestorPosition`, `InvoiceTrancheExposure`) — could fall
//! below the persistent-entry threshold and get archived, after which the
//! pool's accounting silently read back as default/missing values.
//!
//! Test semantics (mirroring real-network TTL behaviour):
//!
//! * The default test ledger gives every new persistent entry a TTL of
//!   `min_persistent_entry_ttl` (4096 ledgers). [`GAP`] jumps further than
//!   that, so entries that are NOT extended die across the gap — in the test
//!   host, reading an archived entry escalates to a panic, which is exactly
//!   what the pre-fix contract did after any idle period.
//! * [`GAP`] is far below the fix's 30-day `INSTANCE_BUMP_AMOUNT`, so with the
//!   fix every entrypoint keeps the storage alive for at least a month after
//!   each call — the production guarantee "the pool survives as long as it is
//!   used at least once a month".
//! * Only the tranche contract's own storage is under test. The companion
//!   token contracts are "kept alive" across the gap (as constantly-used
//!   tokens would be on the network) via [`keep_alive`]; the tranche contract
//!   itself is deliberately NOT propped up — surviving the gap is the fix.

use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::Ledger as _;
use soroban_sdk::{contract, contractimpl, Address, Env, Map, Symbol};
use tranche::state::TrancheClass;
use tranche::TrancheContract;

/// Ledger gap between two interactions: larger than the default 4096-ledger
/// persistent TTL (so un-extended entries expire) and much smaller than the
/// fix's 30-day bump-to value.
const GAP: u32 = 5000;

fn advance_ledgers(env: &Env, ledgers: u32) {
    let seq = env.ledger().sequence();
    env.ledger().with_mut(|l| l.sequence_number = seq + ledgers);
}

/// Simulates a companion contract that is regularly used on the network
/// (e.g. the token everyone trades): its instance TTL is extended well past
/// the gap. Must be called while the contract is still live — extending an
/// already-archived entry is impossible, on the test host and on the network.
fn keep_alive(env: &Env, contract_id: &Address) {
    env.as_contract(contract_id, || {
        env.storage().instance().extend_ttl(GAP * 2, GAP * 2);
    });
}

/// Minimal token used instead of the Stellar Asset Contract so the whole
/// token state lives in one instance entry that `keep_alive` can cover.
/// `deposit_tranche` invokes its `transfer` via `token::Client`, which only
/// requires a contract exposing that function signature.
#[contract]
pub struct FakeToken;

#[contractimpl]
impl FakeToken {
    fn balances(env: &Env) -> Map<Address, i128> {
        env.storage()
            .instance()
            .get(&Symbol::new(env, "balances"))
            .unwrap_or_else(|| Map::new(env))
    }

    pub fn faucet(env: Env, to: Address, amount: i128) {
        let mut balances = Self::balances(&env);
        let current = balances.get(to.clone()).unwrap_or(0);
        balances.set(to, current + amount);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "balances"), &balances);
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        let mut balances = Self::balances(&env);
        let from_balance = balances.get(from.clone()).unwrap_or(0);
        assert!(from_balance >= amount, "insufficient token balance");
        balances.set(from.clone(), from_balance - amount);
        let to_balance = balances.get(to.clone()).unwrap_or(0);
        balances.set(to, to_balance + amount);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "balances"), &balances);
    }
}

/// Share token whose state also lives entirely in instance storage.
#[contract]
pub struct DummyShare;

#[contractimpl]
impl DummyShare {
    fn supply(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get(&Symbol::new(env, "tot"))
            .unwrap_or(0)
    }

    fn balances(env: &Env) -> Map<Address, i128> {
        env.storage()
            .instance()
            .get(&Symbol::new(env, "bals"))
            .unwrap_or_else(|| Map::new(env))
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        let total = Self::supply(&env);
        let mut balances = Self::balances(&env);
        let current = balances.get(to.clone()).unwrap_or(0);
        balances.set(to, current + amount);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "tot"), &(total + amount));
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "bals"), &balances);
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        let total = Self::supply(&env);
        let mut balances = Self::balances(&env);
        let current = balances.get(from.clone()).unwrap_or(0);
        assert!(current >= amount, "insufficient share balance");
        balances.set(from, current - amount);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "tot"), &(total - amount));
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "bals"), &balances);
    }
}

struct Setup {
    env: Env,
    contract_id: Address,
    token: Address,
    senior_share_token: Address,
    junior_share_token: Address,
    admin: Address,
    investor_senior: Address,
    investor_junior: Address,
}

fn setup() -> Setup {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token = env.register(FakeToken, ());
    let senior_share_token = env.register(DummyShare, ());
    let junior_share_token = env.register(DummyShare, ());
    let investor_senior = Address::generate(&env);
    let investor_junior = Address::generate(&env);

    env.as_contract(&token, || {
        FakeToken::faucet(env.clone(), investor_senior.clone(), 1_000_000);
        FakeToken::faucet(env.clone(), investor_junior.clone(), 1_000_000);
    });

    let contract_id = env.register(TrancheContract, ());

    env.as_contract(&contract_id, || {
        TrancheContract::initialize(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_token.clone(),
            junior_share_token.clone(),
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        );
    });

    let senior_share_arg = senior_share_token.clone();
    let junior_share_arg = junior_share_token.clone();
    env.as_contract(&contract_id, || {
        TrancheContract::open_tranche_for_token(
            env.clone(),
            admin.clone(),
            token.clone(),
            senior_share_arg,
            junior_share_arg,
            tranche::state::TrancheConfig {
                senior_target_yield_bps: 1000,
                senior_advance_rate_bps: 8000,
                junior_first_loss_bps: 10000,
            },
        )
        .unwrap();
    });

    Setup {
        env,
        contract_id,
        token,
        senior_share_token,
        junior_share_token,
        admin,
        investor_senior,
        investor_junior,
    }
}

/// Deposits survive TTL windows: positions written long ago still read back
/// with their exact values, and the pool keeps accepting deposits that
/// compose correctly with the pre-gap accounting.
#[test]
fn test_pool_state_survives_ttl_windows() {
    let Setup {
        env,
        contract_id,
        token,
        senior_share_token,
        junior_share_token,
        investor_senior,
        investor_junior,
        ..
    } = setup();

    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor_junior.clone(),
            token.clone(),
            TrancheClass::Junior,
            2000,
        );
    });
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor_senior.clone(),
            token.clone(),
            TrancheClass::Senior,
            5000,
        );
    });
    assert_eq!(
        env.as_contract(&senior_share_token, || DummyShare::supply(&env)),
        5000
    );

    // Keep the companion tokens alive (as network usage would); the tranche
    // contract must survive on its own — that is the fix under test.
    keep_alive(&env, &token);
    keep_alive(&env, &senior_share_token);
    keep_alive(&env, &junior_share_token);

    advance_ledgers(&env, GAP);

    // Previously: archived entries read back as default/missing.
    env.as_contract(&contract_id, || {
        let pool = TrancheContract::get_pool(env.clone(), token.clone());
        assert_eq!(pool.senior.deposited, 5000);
        assert_eq!(pool.junior.deposited, 2000);

        let senior_pos = TrancheContract::get_position(
            env.clone(),
            investor_senior.clone(),
            token.clone(),
            TrancheClass::Senior,
        );
        let senior_pos = senior_pos.expect("senior position must survive TTL windows");
        assert_eq!(senior_pos.deposited, 5000);
        assert_eq!(senior_pos.shares, 5000);

        let junior_pos = TrancheContract::get_position(
            env.clone(),
            investor_junior.clone(),
            token.clone(),
            TrancheClass::Junior,
        );
        let junior_pos = junior_pos.expect("junior position must survive TTL windows");
        assert_eq!(junior_pos.deposited, 2000);
    });

    // The pool keeps working after the gap: the advance-rate check still sees
    // the pre-gap accounting (2000 junior supports 8000 senior; 5000 already
    // deposited, so exactly 3000 more fit). A fresh investor mints/deposits.
    let investor2 = Address::generate(&env);
    env.as_contract(&token, || {
        FakeToken::faucet(env.clone(), investor2.clone(), 1_000_000);
    });
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor2.clone(),
            token.clone(),
            TrancheClass::Senior,
            3000,
        );
    });
    env.as_contract(&contract_id, || {
        let pool = TrancheContract::get_pool(env.clone(), token.clone());
        assert_eq!(pool.senior.deposited, 8000);
    });
}

/// Funding and the resulting invoice exposure survive TTL windows, and a
/// repayment distributed long afterwards still splits on the recorded
/// exposure instead of starting from zero.
#[test]
fn test_exposure_survives_ttl_windows_and_repay_works() {
    let Setup {
        env,
        contract_id,
        token,
        investor_senior,
        investor_junior,
        ..
    } = setup();

    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor_junior.clone(),
            token.clone(),
            TrancheClass::Junior,
            2000,
        );
    });
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor_senior.clone(),
            token.clone(),
            TrancheClass::Senior,
            8000,
        );
    });
    env.as_contract(&contract_id, || {
        let (senior_amt, junior_amt) =
            TrancheContract::fund_invoice_from_tranches(env.clone(), token.clone(), 1, 1000);
        assert_eq!(senior_amt, 800);
        assert_eq!(junior_amt, 200);
    });

    keep_alive(&env, &token);
    advance_ledgers(&env, GAP);

    // The exposure recorded before the gap must still be there.
    env.as_contract(&contract_id, || {
        let exposure = TrancheContract::get_invoice_exposure(env.clone(), 1);
        let exposure = exposure.expect("invoice exposure must survive TTL windows");
        assert_eq!(exposure.senior_deployed, 800);
        assert_eq!(exposure.junior_deployed, 200);

        let pool = TrancheContract::get_pool(env.clone(), token.clone());
        assert_eq!(pool.senior.deployed, 800);
        assert_eq!(pool.junior.deployed, 200);
    });

    // Repaying after the gap still splits on the recorded exposure
    // (800 senior principal + ~10% yield on 30 days ≈ 806.58).
    env.as_contract(&contract_id, || {
        let (senior_payout, junior_payout) = TrancheContract::distribute_waterfall_repayment(
            env.clone(),
            token.clone(),
            1,
            1100,
            30 * 24 * 60 * 60,
        );
        assert!(senior_payout > 800 && senior_payout <= 810);
        assert!(junior_payout > 200);
    });

    env.as_contract(&contract_id, || {
        let pool = TrancheContract::get_pool(env.clone(), token.clone());
        assert_eq!(pool.senior.deployed, 0);
        assert_eq!(pool.junior.deployed, 0);
        assert!(pool.senior.earned > 0);
    });
}

/// Admin state survives TTL windows: the stored admin, the enabled flag and
/// the pool config all remain readable and the pool stays administrable.
#[test]
fn test_admin_config_survive_ttl_windows() {
    let Setup {
        env,
        contract_id,
        token,
        senior_share_token,
        junior_share_token,
        admin,
        investor_senior,
        ..
    } = setup();

    // Companion tokens stay in use on the network; the tranche contract is
    // deliberately not propped up — surviving the gap is the fix under test.
    keep_alive(&env, &token);
    keep_alive(&env, &senior_share_token);
    keep_alive(&env, &junior_share_token);
    advance_ledgers(&env, GAP);

    env.as_contract(&contract_id, || {
        assert_eq!(
            TrancheContract::get_admin(env.clone()).ok(),
            Some(admin.clone())
        );
        assert!(TrancheContract::is_tranche_enabled(
            env.clone(),
            token.clone()
        ));

        // Config update after the gap reads the pool that was written before
        // it (previously PoolNotFound).
        assert!(TrancheContract::set_tranche_config(
            env.clone(),
            admin.clone(),
            token.clone(),
            1200,
            8000,
            10000,
        )
        .is_ok());

        let config = TrancheContract::get_config(env.clone(), token.clone());
        assert_eq!(config.senior_target_yield_bps, 1200);
    });

    // And the pool is still fully usable with the new config.
    env.as_contract(&token, || {
        FakeToken::faucet(env.clone(), investor_senior.clone(), 1_000_000);
    });
    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor_senior.clone(),
            token.clone(),
            TrancheClass::Junior,
            2000,
        );
    });
}

/// `get_advance_rate_headroom` (a pure read of pre-gap accounting) stays
/// correct across TTL windows.
#[test]
fn test_headroom_view_survives_ttl_windows() {
    let Setup {
        env,
        contract_id,
        token,
        investor_junior,
        ..
    } = setup();

    env.as_contract(&contract_id, || {
        TrancheContract::deposit_tranche(
            env.clone(),
            investor_junior.clone(),
            token.clone(),
            TrancheClass::Junior,
            2000,
        );
    });

    keep_alive(&env, &token);
    advance_ledgers(&env, GAP);

    env.as_contract(&contract_id, || {
        let headroom = TrancheContract::get_advance_rate_headroom(env.clone(), token.clone());
        assert_eq!(headroom, 8000);
    });
}
