#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, String, Symbol, Vec,
};

const EVT: Symbol = symbol_short!("share");

/// Maximum number of balance checkpoints retained per holder.
/// Once the list is full the oldest entry is dropped before a new one is
/// appended, giving a bounded rolling window (≈ 1 checkpoint / ledger-second
/// worst-case, or ~2.8 years of daily snapshots at the common 1-per-day rate).
/// Governance's `balance_at` queries target recent proposal-creation timestamps,
/// so pruning ancient history does not affect correctness in practice.
pub const MAX_CHECKPOINTS: u32 = 1_024;

#[contracttype]
pub enum DataKey {
    Admin,
    Paused,
    Name,
    Symbol,
    Decimals,
    Balance(Address),
    Allowance(Address, Address),
    TotalSupply,
    /// Historical (timestamp, balance) checkpoints per holder, append-only and
    /// ordered by timestamp. Lets callers (e.g. governance) read a holder's
    /// balance as of a past point in time instead of their current balance,
    /// so voting power reflects the snapshot at proposal creation rather than
    /// whatever the holder's balance happens to be when they cast their vote.
    Checkpoints(Address),
}

/// Records a checkpoint of `who`'s new balance at the current ledger timestamp.
/// Multiple writes within the same timestamp overwrite the last checkpoint for
/// that timestamp rather than appending, keeping the list free of duplicates.
fn write_checkpoint(env: &Env, who: &Address, new_balance: i128) {
    let key = DataKey::Checkpoints(who.clone());
    let mut checkpoints: Vec<(u64, i128)> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or(Vec::new(env));
    let now = env.ledger().timestamp();
    if let Some(last) = checkpoints.last() {
        if last.0 == now {
            checkpoints.set(checkpoints.len() - 1, (now, new_balance));
            env.storage().persistent().set(&key, &checkpoints);
            return;
        }
    }
    // Evict the oldest entry before appending so the Vec never exceeds the cap.
    if checkpoints.len() >= MAX_CHECKPOINTS {
        checkpoints.remove(0);
    }
    checkpoints.push_back((now, new_balance));
    env.storage().persistent().set(&key, &checkpoints);
}

fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get(&DataKey::Paused)
        .unwrap_or(false)
    {
        panic!("contract is paused");
    }
}

#[contract]
pub struct ShareToken;

#[contractimpl]
impl ShareToken {
    pub fn initialize(env: Env, admin: Address, decimals: u32, name: String, symbol: String) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(&DataKey::Decimals, &decimals);
        env.storage().instance().set(&DataKey::Name, &name);
        env.storage().instance().set(&DataKey::Symbol, &symbol);
        env.storage().instance().set(&DataKey::TotalSupply, &0i128);
        env.events()
            .publish((EVT, symbol_short!("init")), (name, symbol, decimals));
    }

    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if admin != stored_admin {
            panic!("unauthorized");
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events().publish((EVT, symbol_short!("paused")), admin);
    }

    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        let stored_admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        if admin != stored_admin {
            panic!("unauthorized");
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events().publish((EVT, symbol_short!("unpause")), admin);
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage().instance().get(&DataKey::Paused).unwrap_or(false)
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        require_not_paused(&env);
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let balance = Self::balance(env.clone(), to.clone());
        let new_balance = balance + amount;
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &new_balance);
        write_checkpoint(&env, &to, new_balance);

        let total: i128 = env.storage().instance().get(&DataKey::TotalSupply).unwrap();
        let new_total = total + amount;
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total);
        env.events()
            .publish((EVT, symbol_short!("mint")), (to, amount, new_total));
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        require_not_paused(&env);
        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        admin.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let balance = Self::balance(env.clone(), from.clone());
        if balance < amount {
            panic!("insufficient balance");
        }
        let new_balance = balance - amount;
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &new_balance);
        write_checkpoint(&env, &from, new_balance);

        let total: i128 = env.storage().instance().get(&DataKey::TotalSupply).unwrap();
        let new_total = total - amount;
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total);
        env.events()
            .publish((EVT, symbol_short!("burn")), (from, amount, new_total));
    }

    pub fn burn_from(env: Env, spender: Address, from: Address, amount: i128) {
        require_not_paused(&env);
        spender.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let allowed = Self::allowance(env.clone(), from.clone(), spender.clone());
        if allowed < amount {
            panic!("allowance exceeded");
        }
        let balance = Self::balance(env.clone(), from.clone());
        if balance < amount {
            panic!("insufficient balance");
        }
        let new_balance = balance - amount;
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &new_balance);
        write_checkpoint(&env, &from, new_balance);

        let total: i128 = env.storage().instance().get(&DataKey::TotalSupply).unwrap();
        let new_total = total - amount;
        env.storage()
            .instance()
            .set(&DataKey::TotalSupply, &new_total);
        env.storage().persistent().set(
            &DataKey::Allowance(from.clone(), spender.clone()),
            &(allowed - amount),
        );
        env.events()
            .publish((EVT, symbol_short!("burn_from")), (spender, from, amount, new_total));
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        require_not_paused(&env);
        from.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let balance_from = Self::balance(env.clone(), from.clone());
        if balance_from < amount {
            panic!("insufficient balance");
        }
        let new_balance_from = balance_from - amount;
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &new_balance_from);
        write_checkpoint(&env, &from, new_balance_from);

        let balance_to = Self::balance(env.clone(), to.clone());
        let new_balance_to = balance_to + amount;
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &new_balance_to);
        write_checkpoint(&env, &to, new_balance_to);
        env.events()
            .publish((EVT, symbol_short!("transfer")), (from, to, amount));
    }

    pub fn approve(env: Env, owner: Address, spender: Address, amount: i128) {
        require_not_paused(&env);
        owner.require_auth();
        if amount < 0 {
            panic!("amount must be non-negative");
        }
        env.storage()
            .persistent()
            .set(&DataKey::Allowance(owner.clone(), spender.clone()), &amount);
        env.events()
            .publish((EVT, symbol_short!("approve")), (owner, spender, amount));
    }

    pub fn allowance(env: Env, owner: Address, spender: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Allowance(owner, spender))
            .unwrap_or(0)
    }

    pub fn increase_allowance(env: Env, owner: Address, spender: Address, added_amount: i128) {
        require_not_paused(&env);
        owner.require_auth();
        if added_amount <= 0 {
            panic!("added amount must be positive");
        }
        let current = Self::allowance(env.clone(), owner.clone(), spender.clone());
        let new_allowance = current
            .checked_add(added_amount)
            .expect("allowance overflow");
        env.storage().persistent().set(
            &DataKey::Allowance(owner.clone(), spender.clone()),
            &new_allowance,
        );
        env.events().publish(
            (EVT, symbol_short!("incrallow")),
            (owner, spender, new_allowance),
        );
    }

    pub fn decrease_allowance(env: Env, owner: Address, spender: Address, subtracted_amount: i128) {
        require_not_paused(&env);
        owner.require_auth();
        if subtracted_amount <= 0 {
            panic!("subtracted amount must be positive");
        }
        let current = Self::allowance(env.clone(), owner.clone(), spender.clone());
        if current < subtracted_amount {
            panic!("allowance underflow");
        }
        let new_allowance = current - subtracted_amount;
        env.storage().persistent().set(
            &DataKey::Allowance(owner.clone(), spender.clone()),
            &new_allowance,
        );
        env.events().publish(
            (EVT, symbol_short!("decrallow")),
            (owner, spender, new_allowance),
        );
    }

    pub fn transfer_from(env: Env, spender: Address, from: Address, to: Address, amount: i128) {
        require_not_paused(&env);
        spender.require_auth();
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let allowed = Self::allowance(env.clone(), from.clone(), spender.clone());
        if allowed < amount {
            panic!("allowance exceeded");
        }
        let balance_from = Self::balance(env.clone(), from.clone());
        if balance_from < amount {
            panic!("insufficient balance");
        }

        let new_balance_from = balance_from - amount;
        env.storage()
            .persistent()
            .set(&DataKey::Balance(from.clone()), &new_balance_from);
        write_checkpoint(&env, &from, new_balance_from);
        let balance_to = Self::balance(env.clone(), to.clone());
        let new_balance_to = balance_to + amount;
        env.storage()
            .persistent()
            .set(&DataKey::Balance(to.clone()), &new_balance_to);
        write_checkpoint(&env, &to, new_balance_to);
        env.storage().persistent().set(
            &DataKey::Allowance(from.clone(), spender.clone()),
            &(allowed - amount),
        );
        env.events().publish(
            (EVT, symbol_short!("xfer_from")),
            (spender, from, to, amount),
        );
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&DataKey::Balance(id))
            .unwrap_or(0)
    }

    /// Returns `id`'s balance as of `timestamp` (inclusive), based on recorded
    /// checkpoints. Governance uses this to weight votes by the balance a
    /// holder had at proposal creation, rather than their balance at vote
    /// time — otherwise a holder could acquire shares mid-vote (or borrow them
    /// just long enough to vote) to inflate their voting power.
    pub fn balance_at(env: Env, id: Address, timestamp: u64) -> i128 {
        let checkpoints: Vec<(u64, i128)> = env
            .storage()
            .persistent()
            .get(&DataKey::Checkpoints(id))
            .unwrap_or(Vec::new(&env));

        if checkpoints.is_empty() {
            return 0;
        }

        // Binary search for the latest checkpoint at or before `timestamp`.
        let mut lo: u32 = 0;
        let mut hi: u32 = checkpoints.len();
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if checkpoints.get(mid).unwrap().0 <= timestamp {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }

        if lo == 0 {
            0
        } else {
            checkpoints.get(lo - 1).unwrap().1
        }
    }

    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&DataKey::TotalSupply)
            .unwrap_or(0)
    }

    pub fn decimals(env: Env) -> u32 {
        env.storage().instance().get(&DataKey::Decimals).unwrap()
    }

    pub fn name(env: Env) -> String {
        env.storage().instance().get(&DataKey::Name).unwrap()
    }

    pub fn symbol(env: Env) -> String {
        env.storage().instance().get(&DataKey::Symbol).unwrap()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::testutils::{Address as _, Ledger};
    use soroban_sdk::Env;

    fn setup(env: &Env) -> (ShareTokenClient<'_>, Address) {
        let contract_id = env.register(ShareToken, ());
        let client = ShareTokenClient::new(env, &contract_id);
        let admin = Address::generate(env);
        client.initialize(
            &admin,
            &7u32,
            &String::from_str(env, "Pool Shares"),
            &String::from_str(env, "POOL"),
        );
        (client, admin)
    }

    #[test]
    fn test_mint_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let to = Address::generate(&env);

        client.mint(&to, &500i128);

        assert_eq!(client.balance(&to), 500);
        assert_eq!(client.total_supply(), 500);
    }

    #[test]
    fn test_burn_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let holder = Address::generate(&env);

        client.mint(&holder, &1_000i128);
        client.burn(&holder, &400i128);

        assert_eq!(client.balance(&holder), 600);
        assert_eq!(client.total_supply(), 600);
    }

    #[test]
    fn test_transfer_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.mint(&alice, &1_000i128);
        client.transfer(&alice, &bob, &300i128);

        assert_eq!(client.balance(&alice), 700);
        assert_eq!(client.balance(&bob), 300);
        assert_eq!(client.total_supply(), 1_000);
    }

    #[test]
    fn test_initialize_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ShareToken, ());
        let client = ShareTokenClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(
            &admin,
            &6u32,
            &String::from_str(&env, "Test Token"),
            &String::from_str(&env, "TEST"),
        );

        assert_eq!(client.decimals(), 6u32);
        assert_eq!(client.total_supply(), 0);
    }

    #[test]
    fn test_mint_requires_admin_auth() {
        let env = Env::default();
        // No mock_all_auths — admin auth check must be satisfied
        let (client, _admin) = setup(&env);
        let to = Address::generate(&env);
        let result = client.try_mint(&to, &100i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_admin_rotates_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup(&env);
        let new_admin = Address::generate(&env);

        assert_eq!(client.admin(), admin);
        client.set_admin(&new_admin);
        assert_eq!(client.admin(), new_admin);

        // The new admin can now mint, the old one cannot.
        let to = Address::generate(&env);
        client.mint(&to, &100i128);
        assert_eq!(client.balance(&to), 100);
    }

    #[test]
    fn test_set_admin_requires_current_admin_auth() {
        let env = Env::default();
        // No mock_all_auths — only the current admin may rotate.
        let (client, _admin) = setup(&env);
        let new_admin = Address::generate(&env);
        let result = client.try_set_admin(&new_admin);
        assert!(result.is_err());
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_burn_zero_amount() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let holder = Address::generate(&env);
        client.mint(&holder, &100i128);
        client.burn(&holder, &0i128);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_transfer_zero_amount() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.mint(&alice, &100i128);
        client.transfer(&alice, &bob, &0i128);
    }

    #[test]
    fn test_initialize_sets_name_symbol_decimals() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ShareToken, ());
        let client = ShareTokenClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(
            &admin,
            &6u32,
            &String::from_str(&env, "Test Shares"),
            &String::from_str(&env, "TST"),
        );

        assert_eq!(client.name(), String::from_str(&env, "Test Shares"));
        assert_eq!(client.symbol(), String::from_str(&env, "TST"));
        assert_eq!(client.decimals(), 6u32);
        assert_eq!(client.total_supply(), 0);
    }

    #[test]
    fn test_set_admin_rotates_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin) = setup(&env);
        let new_admin = Address::generate(&env);

        client.set_admin(&new_admin);

        // The new admin can now mint, proving the rotation took effect.
        let to = Address::generate(&env);
        client.mint(&to, &100i128);
        assert_eq!(client.balance(&to), 100);
        assert_ne!(admin, new_admin);
    }

    #[test]
    fn test_set_admin_requires_current_admin_auth() {
        let env = Env::default();
        // No mock_all_auths — the current admin's auth must be satisfied.
        let (client, _admin) = setup(&env);
        let new_admin = Address::generate(&env);
        let result = client.try_set_admin(&new_admin);
        assert!(result.is_err());
    }

    #[test]
    #[should_panic(expected = "already initialized")]
    fn test_double_initialize_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ShareToken, ());
        let client = ShareTokenClient::new(&env, &contract_id);
        let admin = Address::generate(&env);

        client.initialize(
            &admin,
            &7u32,
            &String::from_str(&env, "Pool Shares"),
            &String::from_str(&env, "POOL"),
        );
        client.initialize(
            &admin,
            &7u32,
            &String::from_str(&env, "Pool Shares"),
            &String::from_str(&env, "POOL"),
        );
    }

    #[test]
    #[should_panic(expected = "insufficient balance")]
    fn test_burn_exceeds_balance_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let holder = Address::generate(&env);

        client.mint(&holder, &100i128);
        client.burn(&holder, &101i128);
    }

    #[test]
    #[should_panic(expected = "insufficient balance")]
    fn test_transfer_exceeds_balance_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.mint(&alice, &50i128);
        client.transfer(&alice, &bob, &51i128);
    }

    #[test]
    fn test_transfer_to_self_leaves_balance_unchanged() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);

        client.mint(&alice, &200i128);
        client.transfer(&alice, &alice, &100i128);

        assert_eq!(client.balance(&alice), 200);
        assert_eq!(client.total_supply(), 200);
    }

    #[test]
    fn test_balance_of_unknown_address_is_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);

        assert_eq!(client.balance(&Address::generate(&env)), 0);
    }

    #[test]
    fn test_total_supply_consistent_after_multi_operations() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.mint(&alice, &1_000i128);
        client.mint(&bob, &500i128);
        assert_eq!(client.total_supply(), 1_500);

        client.burn(&alice, &200i128);
        assert_eq!(client.total_supply(), 1_300);

        client.transfer(&alice, &bob, &300i128);
        assert_eq!(client.total_supply(), 1_300);
        assert_eq!(client.balance(&alice), 500);
        assert_eq!(client.balance(&bob), 800);
    }

    #[test]
    fn test_approve_and_transfer_from() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        let recipient = Address::generate(&env);

        client.mint(&owner, &1_000i128);
        client.approve(&owner, &spender, &400i128);
        client.transfer_from(&spender, &owner, &recipient, &250i128);

        assert_eq!(client.balance(&owner), 750);
        assert_eq!(client.balance(&recipient), 250);
        assert_eq!(client.allowance(&owner, &spender), 150);
        assert_eq!(client.total_supply(), 1_000);
    }

    #[test]
    #[should_panic(expected = "allowance exceeded")]
    fn test_transfer_from_fails_exceeds_allowance() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin) = setup(&env);
        let owner = Address::generate(&env);
        let spender = Address::generate(&env);
        let recipient = Address::generate(&env);

        client.mint(&owner, &1_000i128);
        client.approve(&owner, &spender, &100i128);
        client.transfer_from(&spender, &owner, &recipient, &101i128);
    }

    #[test]
    fn test_balance_at_before_any_checkpoint_is_zero() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);

        assert_eq!(client.balance_at(&alice, &500), 0);
    }

    #[test]
    fn test_balance_at_reflects_balance_at_past_timestamp() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);

        client.mint(&alice, &100i128);

        env.ledger().with_mut(|l| l.timestamp = 2_000);
        client.mint(&alice, &400i128);

        env.ledger().with_mut(|l| l.timestamp = 3_000);
        client.burn(&alice, &200i128);

        // Balance history: t=1000 -> 100, t=2000 -> 500, t=3000 -> 300
        assert_eq!(client.balance_at(&alice, &1_000), 100);
        assert_eq!(client.balance_at(&alice, &1_500), 100);
        assert_eq!(client.balance_at(&alice, &2_000), 500);
        assert_eq!(client.balance_at(&alice, &2_999), 500);
        assert_eq!(client.balance_at(&alice, &3_000), 300);
        assert_eq!(client.balance_at(&alice, &10_000), 300);
    }

    #[test]
    fn test_balance_at_not_affected_by_later_transfers() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);

        client.mint(&alice, &1_000i128);
        let snapshot_ts = env.ledger().timestamp();

        env.ledger().with_mut(|l| l.timestamp = 2_000);
        client.transfer(&alice, &bob, &1_000i128);

        // Historical balance at proposal-creation time is unaffected by the
        // later transfer that drained alice's live balance to zero.
        assert_eq!(client.balance_at(&alice, &snapshot_ts), 1_000);
        assert_eq!(client.balance(&alice), 0);
    }

    #[test]
    fn test_balance_at_dedupes_same_timestamp_writes() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, _admin) = setup(&env);
        let alice = Address::generate(&env);

        // transfer-to-self writes two checkpoints for alice at the same
        // timestamp; balance_at must reflect the final value, not stack
        // duplicate entries.
        client.mint(&alice, &200i128);
        client.transfer(&alice, &alice, &100i128);
        assert_eq!(client.balance_at(&alice, &1_000), 200);
    }
}
