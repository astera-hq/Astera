#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, BytesN, Env, String, Symbol,
};

const LEDGERS_PER_DAY: u32 = 17_280;
const ACTIVE_INVOICE_TTL: u32 = LEDGERS_PER_DAY * 365;
const COMPLETED_INVOICE_TTL: u32 = LEDGERS_PER_DAY * 30;
const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 30;
const INSTANCE_LIFETIME_THRESHOLD: u32 = LEDGERS_PER_DAY * 7;
const UPGRADE_TIMELOCK_SECS: u64 = 86400; // 24 hours
const MAX_INVOICES_PER_DAY: u32 = 10;
const SECS_PER_DAY: u64 = 86400;
const DEFAULT_GRACE_PERIOD_DAYS: u32 = 7; // 7 days default grace period

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum InvoiceStatus {
    Pending,
    AwaitingVerification,
    Verified,
    Disputed,
    Funded,
    Paid,
    Defaulted,
}

#[contracttype]
#[derive(Clone)]
pub struct Invoice {
    pub id: u64,
    pub owner: Address,
    pub debtor: String,
    pub amount: i128,
    pub due_date: u64,
    pub description: String,
    pub status: InvoiceStatus,
    pub created_at: u64,
    pub funded_at: u64,
    pub paid_at: u64,
    pub pool_contract: Address,
    pub verification_hash: String,
    pub oracle_verified: bool,
    pub dispute_reason: String,
}

/// Wallet / explorer–oriented view derived from [`Invoice`] (no extra storage).
/// Field names align with common JSON token metadata (`name`, `description`, `image`)
/// plus invoice-specific attributes; see `contracts/invoice/README.md` for SEP notes.
#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub struct InvoiceMetadata {
    pub name: String,
    pub description: String,
    /// Placeholder asset URI until per-invoice art exists.
    pub image: String,
    pub amount: i128,
    pub debtor: String,
    pub due_date: u64,
    pub status: InvoiceStatus,
    /// Short ticker, SEP-0041–style (e.g. `INV-1`).
    pub symbol: String,
    /// Smallest units per whole token for `amount` (USDC on Stellar uses 7).
    pub decimals: u32,
}

#[contracttype]
#[derive(Clone, Default)]
pub struct StorageStats {
    pub total_invoices: u64,
    pub active_invoices: u64,
    pub cleaned_invoices: u64,
}

#[contracttype]
pub enum DataKey {
    Invoice(u64),
    InvoiceCount,
    Admin,
    Pool,
    Oracle,
    Initialized,
    StorageStats,
    Paused,
    DailyInvoiceCount(Address),
    DailyInvoiceResetTime(Address),
    ProposedWasmHash,
    UpgradeScheduledAt,
    GracePeriodDays,
}

const EVT: Symbol = symbol_short!("INVOICE");

fn bump_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_LIFETIME_THRESHOLD, INSTANCE_BUMP_AMOUNT);
}

fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        panic!("contract is paused");
    }
}

fn set_invoice_ttl(env: &Env, id: u64, is_completed: bool) {
    let ttl = if is_completed {
        COMPLETED_INVOICE_TTL
    } else {
        ACTIVE_INVOICE_TTL
    };
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::Invoice(id), ttl, ttl);
}

/// Writes decimal digits of `n` into `buf` (left-aligned), returns digit count.
fn write_u64_decimal(buf: &mut [u8], mut n: u64) -> usize {
    if n == 0 {
        if buf.is_empty() {
            return 0;
        }
        buf[0] = b'0';
        return 1;
    }
    let mut i = 0usize;
    while n > 0 {
        if i >= buf.len() {
            break;
        }
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    let mut lo = 0usize;
    let mut hi = i - 1;
    while lo < hi {
        buf.swap(lo, hi);
        lo += 1;
        hi -= 1;
    }
    i
}

fn concat_prefix_u64(env: &Env, prefix: &[u8], id: u64) -> String {
    let mut buf = [0u8; 40];
    let plen = prefix.len();
    buf[..plen].copy_from_slice(prefix);
    let dlen = write_u64_decimal(&mut buf[plen..], id);
    String::from_bytes(env, &buf[..plen + dlen])
}

#[contract]
pub struct InvoiceContract;

#[contractimpl]
impl InvoiceContract {
    pub fn initialize(env: Env, admin: Address, pool: Address) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Pool, &pool);
        env.storage().instance().set(&DataKey::InvoiceCount, &0u64);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage()
            .instance()
            .set(&DataKey::StorageStats, &StorageStats::default());
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::GracePeriodDays, &DEFAULT_GRACE_PERIOD_DAYS);
        bump_instance(&env);
    }

    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized");
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        bump_instance(&env);
        env.events().publish((EVT, symbol_short!("paused")), admin);
    }

    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized");
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        bump_instance(&env);
        env.events()
            .publish((EVT, symbol_short!("unpaused")), admin);
    }

    pub fn is_paused(env: Env) -> bool {
        bump_instance(&env);
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }

    pub fn set_oracle(env: Env, admin: Address, oracle: Address) {
        admin.require_auth();
        require_not_paused(&env);
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized");
        }
        env.storage().instance().set(&DataKey::Oracle, &oracle);
        bump_instance(&env);
        env.events()
            .publish((EVT, symbol_short!("set_orc")), (admin, oracle));
    }

    pub fn create_invoice(
        env: Env,
        owner: Address,
        debtor: String,
        amount: i128,
        due_date: u64,
        description: String,
        verification_hash: String,
    ) -> u64 {
        owner.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        if amount <= 0 {
            panic!("amount must be positive");
        }
        if due_date <= env.ledger().timestamp() {
            panic!("due date must be in the future");
        }

        // Rate limiting: max 10 invoices per day per address
        let now = env.ledger().timestamp();
        let daily_count_key = DataKey::DailyInvoiceCount(owner.clone());
        let daily_reset_key = DataKey::DailyInvoiceResetTime(owner.clone());

        let reset_time: u64 = env.storage().instance().get(&daily_reset_key).unwrap_or(0);
        let mut daily_count: u32 = env.storage().instance().get(&daily_count_key).unwrap_or(0);

        if now >= reset_time + SECS_PER_DAY {
            // Reset the daily counter
            daily_count = 0;
            env.storage().instance().set(&daily_reset_key, &now);
        }

        if daily_count >= MAX_INVOICES_PER_DAY {
            panic!("daily invoice limit exceeded: max 10 per day");
        }

        daily_count += 1;
        env.storage().instance().set(&daily_count_key, &daily_count);

        let count: u64 = env
            .storage()
            .instance()
            .get(&DataKey::InvoiceCount)
            .unwrap_or(0);
        let id = count + 1;

        let pool_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::Pool)
            .expect("pool not configured");
        let empty_str = String::from_str(&env, "");

        let has_oracle = env.storage().instance().has(&DataKey::Oracle);
        let initial_status = if has_oracle {
            InvoiceStatus::AwaitingVerification
        } else {
            InvoiceStatus::Pending
        };

        let invoice = Invoice {
            id,
            owner: owner.clone(),
            debtor,
            amount,
            due_date,
            description,
            status: initial_status,
            created_at: env.ledger().timestamp(),
            funded_at: 0,
            paid_at: 0,
            pool_contract: pool_addr,
            verification_hash,
            oracle_verified: false,
            dispute_reason: empty_str,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);
        set_invoice_ttl(&env, id, false);
        env.storage().instance().set(&DataKey::InvoiceCount, &id);

        let mut stats: StorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        stats.total_invoices += 1;
        stats.active_invoices += 1;
        env.storage().instance().set(&DataKey::StorageStats, &stats);

        env.events()
            .publish((EVT, symbol_short!("created")), (id, owner, amount));

        id
    }

    pub fn verify_invoice(env: Env, id: u64, oracle: Address, approved: bool, reason: String) {
        oracle.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let stored_oracle: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .expect("oracle not configured");
        if oracle != stored_oracle {
            panic!("unauthorized oracle");
        }

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");

        if invoice.status != InvoiceStatus::AwaitingVerification {
            panic!("invoice is not awaiting verification");
        }

        if approved {
            invoice.status = InvoiceStatus::Verified;
            invoice.oracle_verified = true;
        } else {
            invoice.status = InvoiceStatus::Disputed;
            invoice.dispute_reason = reason;
        }

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);
        set_invoice_ttl(&env, id, false);

        if approved {
            env.events().publish((EVT, symbol_short!("verified")), id);
        } else {
            env.events().publish((EVT, symbol_short!("disputed")), id);
        }
    }

    pub fn resolve_dispute(env: Env, id: u64, admin: Address, approved: bool) {
        admin.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized");
        }

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");

        if invoice.status != InvoiceStatus::Disputed {
            panic!("invoice is not disputed");
        }

        if approved {
            invoice.status = InvoiceStatus::Verified;
            invoice.oracle_verified = true;
            invoice.dispute_reason = String::from_str(&env, "");
            env.events().publish((EVT, symbol_short!("resolved")), id);
        } else {
            invoice.status = InvoiceStatus::Defaulted;
            let mut stats: StorageStats = env
                .storage()
                .instance()
                .get(&DataKey::StorageStats)
                .unwrap_or_default();
            stats.active_invoices = stats.active_invoices.saturating_sub(1);
            env.storage().instance().set(&DataKey::StorageStats, &stats);
            set_invoice_ttl(&env, id, true);
            env.events().publish((EVT, symbol_short!("rejected")), id);
        }

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);
    }

    pub fn mark_funded(env: Env, id: u64, pool: Address) {
        pool.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let authorized_pool: Address = env
            .storage()
            .instance()
            .get(&DataKey::Pool)
            .expect("not initialized");
        if pool != authorized_pool {
            panic!("unauthorized pool");
        }

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");

        let is_fundable =
            invoice.status == InvoiceStatus::Pending || invoice.status == InvoiceStatus::Verified;
        if !is_fundable {
            panic!("invoice is not in fundable state");
        }

        invoice.status = InvoiceStatus::Funded;
        invoice.funded_at = env.ledger().timestamp();
        invoice.pool_contract = pool;

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);
        set_invoice_ttl(&env, id, false);
        env.events().publish((EVT, symbol_short!("funded")), id);
    }

    pub fn mark_paid(env: Env, id: u64, caller: Address) {
        caller.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let pool: Address = env
            .storage()
            .instance()
            .get(&DataKey::Pool)
            .expect("not initialized");
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");

        if caller != invoice.owner && caller != pool && caller != admin {
            panic!("unauthorized");
        }
        if invoice.status != InvoiceStatus::Funded {
            panic!("invoice is not funded");
        }

        invoice.status = InvoiceStatus::Paid;
        invoice.paid_at = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);
        set_invoice_ttl(&env, id, true);

        let mut stats: StorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        stats.active_invoices = stats.active_invoices.saturating_sub(1);
        env.storage().instance().set(&DataKey::StorageStats, &stats);

        env.events().publish((EVT, symbol_short!("paid")), id);
    }

    pub fn mark_defaulted(env: Env, id: u64, pool: Address) {
        pool.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let authorized_pool: Address = env
            .storage()
            .instance()
            .get(&DataKey::Pool)
            .expect("not initialized");
        if pool != authorized_pool {
            panic!("unauthorized pool");
        }

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");

        if invoice.status != InvoiceStatus::Funded {
            panic!("invoice is not funded");
        }

        // Check grace period
        let grace_period_days: u32 = env
            .storage()
            .instance()
            .get(&DataKey::GracePeriodDays)
            .unwrap_or(DEFAULT_GRACE_PERIOD_DAYS);
        let grace_period_secs = grace_period_days as u64 * SECS_PER_DAY;
        let now = env.ledger().timestamp();

        if now < invoice.due_date + grace_period_secs {
            panic!("grace period has not expired");
        }

        invoice.status = InvoiceStatus::Defaulted;
        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);
        set_invoice_ttl(&env, id, true);

        let mut stats: StorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        stats.active_invoices = stats.active_invoices.saturating_sub(1);
        env.storage().instance().set(&DataKey::StorageStats, &stats);

        env.events().publish((EVT, symbol_short!("default")), id);
    }

    pub fn cleanup_invoice(env: Env, id: u64, caller: Address) {
        caller.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if caller != admin {
            panic!("unauthorized");
        }

        let invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");

        let is_completed =
            invoice.status == InvoiceStatus::Paid || invoice.status == InvoiceStatus::Defaulted;
        if !is_completed {
            panic!("can only cleanup completed invoices");
        }

        env.storage().persistent().remove(&DataKey::Invoice(id));

        let mut stats: StorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        stats.cleaned_invoices += 1;
        env.storage().instance().set(&DataKey::StorageStats, &stats);

        env.events().publish((EVT, symbol_short!("cleanup")), id);
    }

    pub fn get_invoice(env: Env, id: u64) -> Invoice {
        bump_instance(&env);
        env.storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found")
    }

    /// SEP-oriented metadata for invoice id `id` (same ledger fields as `get_invoice`).
    pub fn get_metadata(env: Env, id: u64) -> InvoiceMetadata {
        let inv: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");

        let name = concat_prefix_u64(&env, b"Astera Invoice #", inv.id);
        let symbol = concat_prefix_u64(&env, b"INV-", inv.id);
        let image = String::from_str(&env, "https://astera.io/metadata/invoice/placeholder.svg");

        InvoiceMetadata {
            name,
            description: inv.description.clone(),
            image,
            amount: inv.amount,
            debtor: inv.debtor.clone(),
            due_date: inv.due_date,
            status: inv.status.clone(),
            symbol,
            decimals: 7,
        }
    }

    pub fn get_invoice_count(env: Env) -> u64 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::InvoiceCount)
            .unwrap_or(0)
    }

    pub fn get_storage_stats(env: Env) -> StorageStats {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default()
    }

    pub fn set_grace_period(env: Env, admin: Address, days: u32) {
        admin.require_auth();
        bump_instance(&env);
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized");
        }
        if days > 90 {
            panic!("grace period cannot exceed 90 days");
        }
        env.storage()
            .instance()
            .set(&DataKey::GracePeriodDays, &days);
    }

    pub fn get_grace_period(env: Env) -> u32 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::GracePeriodDays)
            .unwrap_or(DEFAULT_GRACE_PERIOD_DAYS)
    }

    pub fn set_pool(env: Env, admin: Address, pool: Address) {
        admin.require_auth();
        require_not_paused(&env);
        bump_instance(&env);
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized");
        }
        env.storage().instance().set(&DataKey::Pool, &pool);
        env.events()
            .publish((EVT, symbol_short!("set_pool")), (admin, pool));
    }

    pub fn propose_upgrade(env: Env, admin: Address, wasm_hash: BytesN<32>) {
        admin.require_auth();
        bump_instance(&env);
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized");
        }
        env.storage()
            .instance()
            .set(&DataKey::ProposedWasmHash, &wasm_hash);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeScheduledAt, &env.ledger().timestamp());
        env.events().publish(
            (EVT, symbol_short!("upg_prop")),
            (admin, env.ledger().timestamp() + UPGRADE_TIMELOCK_SECS),
        );
    }

    pub fn execute_upgrade(env: Env, admin: Address) {
        admin.require_auth();
        bump_instance(&env);
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized");
        }
        let scheduled_at: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeScheduledAt)
            .expect("no upgrade proposed");
        let now = env.ledger().timestamp();
        if now < scheduled_at + UPGRADE_TIMELOCK_SECS {
            panic!("upgrade timelock not expired");
        }
        let wasm_hash: BytesN<32> = env
            .storage()
            .instance()
            .get(&DataKey::ProposedWasmHash)
            .expect("no wasm hash proposed");
        let wasm_hash_bytes: BytesN<32> = wasm_hash.try_into().unwrap();
        env.deployer().update_current_contract_wasm(wasm_hash_bytes);
        env.events()
            .publish((EVT, symbol_short!("upgraded")), (admin, now));
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Env,
    };

    fn setup(env: &Env) -> (InvoiceContractClient<'_>, Address, Address, Address) {
        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let pool = Address::generate(env);
        let sme = Address::generate(env);
        client.initialize(&admin, &pool);
        (client, admin, pool, sme)
    }

    #[test]
    fn test_create_and_fund_invoice() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, pool, sme) = setup(&env);
        let hash = String::from_str(&env, "abc123");

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "ACME Corp"),
            &1_000_000_000i128,
            &(env.ledger().timestamp() + 2_592_000),
            &String::from_str(&env, "Invoice #001 - Goods delivery"),
            &hash,
        );

        assert_eq!(id, 1);

        let invoice = client.get_invoice(&id);
        assert!(matches!(invoice.status, InvoiceStatus::Pending));

        let meta = client.get_metadata(&id);
        assert_eq!(meta.status, InvoiceStatus::Pending);
        assert_eq!(meta.amount, 1_000_000_000i128);
        assert_eq!(meta.decimals, 7u32);
        assert_eq!(meta.symbol, String::from_str(&env, "INV-1"));
        assert_eq!(meta.name, String::from_str(&env, "Astera Invoice #1"));

        client.mark_funded(&id, &pool);
        let invoice = client.get_invoice(&id);
        assert_eq!(invoice.status, InvoiceStatus::Funded);
        assert_eq!(client.get_metadata(&id).status, InvoiceStatus::Funded);

        client.mark_paid(&id, &sme);
        let invoice = client.get_invoice(&id);
        assert_eq!(invoice.status, InvoiceStatus::Paid);
        assert_eq!(client.get_metadata(&id).status, InvoiceStatus::Paid);
    }

    #[test]
    fn test_full_lifecycle_create_fund_pay() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let (client, _admin, pool, owner) = setup(&env);
        let hash = String::from_str(&env, "hash123");

        let due = env.ledger().timestamp() + 86_400;
        let id = client.create_invoice(
            &owner,
            &String::from_str(&env, "Debtor"),
            &500_000_000i128,
            &due,
            &String::from_str(&env, "Lifecycle"),
            &hash,
        );
        assert_eq!(id, 1);
        assert!(matches!(
            client.get_invoice(&id).status,
            InvoiceStatus::Pending
        ));

        client.mark_funded(&id, &pool);
        let inv = client.get_invoice(&id);
        assert!(matches!(inv.status, InvoiceStatus::Funded));
        assert_ne!(inv.funded_at, 0);

        client.mark_paid(&id, &pool);
        let inv = client.get_invoice(&id);
        assert!(matches!(inv.status, InvoiceStatus::Paid));
        assert_ne!(inv.paid_at, 0);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_create_invoice_zero_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let hash = String::from_str(&env, "h");

        client.create_invoice(
            &sme,
            &String::from_str(&env, "X"),
            &0i128,
            &(env.ledger().timestamp() + 1),
            &String::from_str(&env, "d"),
            &hash,
        );
    }

    #[test]
    #[should_panic(expected = "due date must be in the future")]
    fn test_create_invoice_past_due_date_panics() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, _admin, _pool, sme) = setup(&env);
        let hash = String::from_str(&env, "h");

        client.create_invoice(
            &sme,
            &String::from_str(&env, "X"),
            &100i128,
            &999_999,
            &String::from_str(&env, "d"),
            &hash,
        );
    }

    #[test]
    #[should_panic(expected = "unauthorized pool")]
    fn test_mark_funded_unauthorized_pool_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let hash = String::from_str(&env, "h");

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &hash,
        );
        let rogue_pool = Address::generate(&env);
        client.mark_funded(&id, &rogue_pool);
    }

    #[test]
    #[should_panic(expected = "invoice is not in fundable state")]
    fn test_mark_funded_already_funded_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, pool, sme) = setup(&env);
        let hash = String::from_str(&env, "h");

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &hash,
        );
        client.mark_funded(&id, &pool);
        client.mark_funded(&id, &pool);
    }

    #[test]
    #[should_panic(expected = "invoice is not funded")]
    fn test_mark_paid_while_pending_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let hash = String::from_str(&env, "h");

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &hash,
        );
        client.mark_paid(&id, &sme);
    }

    #[test]
    #[should_panic(expected = "invoice is not funded")]
    fn test_mark_defaulted_on_paid_invoice_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, pool, sme) = setup(&env);
        let hash = String::from_str(&env, "h");

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &hash,
        );
        client.mark_funded(&id, &pool);
        client.mark_paid(&id, &sme);
        client.mark_defaulted(&id, &pool);
    }

    #[test]
    #[should_panic(expected = "invoice not found")]
    fn test_get_invoice_nonexistent_id_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, _sme) = setup(&env);

        client.get_invoice(&999u64);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_set_pool_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, _sme) = setup(&env);

        let intruder = Address::generate(&env);
        let new_pool = Address::generate(&env);
        client.set_pool(&intruder, &new_pool);
    }

    #[test]
    fn test_invoice_count_increments_across_creates() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);

        let due = env.ledger().timestamp() + 50_000;
        let d = String::from_str(&env, "D");
        let desc = String::from_str(&env, "i");
        let hash = String::from_str(&env, "h");

        let id1 = client.create_invoice(&sme, &d, &100i128, &due, &desc, &hash);
        let id2 = client.create_invoice(&sme, &d, &200i128, &due, &desc, &hash);
        let id3 = client.create_invoice(&sme, &d, &300i128, &due, &desc, &hash);

        assert_eq!(id1, 1);
        assert_eq!(id2, 2);
        assert_eq!(id3, 3);
        assert_eq!(client.get_invoice_count(), 3);
    }

    #[test]
    fn test_oracle_verification() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);

        let oracle = Address::generate(&env);
        client.set_oracle(&admin, &oracle);

        let hash = String::from_str(&env, "verification_hash");
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "desc"),
            &hash,
        );

        let invoice = client.get_invoice(&id);
        assert_eq!(invoice.status, InvoiceStatus::AwaitingVerification);

        client.verify_invoice(&id, &oracle, &true, &String::from_str(&env, ""));
        let invoice = client.get_invoice(&id);
        assert_eq!(invoice.status, InvoiceStatus::Verified);
        assert!(invoice.oracle_verified);

        client.mark_funded(&id, &pool);
        assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Funded);
    }

    #[test]
    fn test_dispute_resolution() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, sme) = setup(&env);

        let oracle = Address::generate(&env);
        client.set_oracle(&admin, &oracle);

        let hash = String::from_str(&env, "h");
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "desc"),
            &hash,
        );

        let reason = String::from_str(&env, "Invalid invoice data");
        client.verify_invoice(&id, &oracle, &false, &reason);
        let invoice = client.get_invoice(&id);
        assert_eq!(invoice.status, InvoiceStatus::Disputed);

        client.resolve_dispute(&id, &admin, &true);
        let invoice = client.get_invoice(&id);
        assert_eq!(invoice.status, InvoiceStatus::Verified);
    }

    #[test]
    fn test_cleanup_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);
        let hash = String::from_str(&env, "h");

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &hash,
        );
        client.mark_funded(&id, &pool);
        client.mark_paid(&id, &sme);

        let stats_before = client.get_storage_stats();
        assert_eq!(stats_before.total_invoices, 1);
        assert_eq!(stats_before.cleaned_invoices, 0);

        client.cleanup_invoice(&id, &admin);

        let stats_after = client.get_storage_stats();
        assert_eq!(stats_after.cleaned_invoices, 1);
    }

    #[test]
    fn test_storage_stats() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, pool, sme) = setup(&env);
        let hash = String::from_str(&env, "h");

        let stats = client.get_storage_stats();
        assert_eq!(stats.total_invoices, 0);
        assert_eq!(stats.active_invoices, 0);

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &hash,
        );

        let stats = client.get_storage_stats();
        assert_eq!(stats.total_invoices, 1);
        assert_eq!(stats.active_invoices, 1);

        client.mark_funded(&id, &pool);
        client.mark_paid(&id, &sme);

        let stats = client.get_storage_stats();
        assert_eq!(stats.active_invoices, 0);
    }

    // ---- Circuit Breaker Tests ----

    #[test]
    fn test_is_paused_false_after_init() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, _sme) = setup(&env);
        assert!(!client.is_paused());
    }

    #[test]
    fn test_pause_and_unpause() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);

        client.pause(&admin);
        assert!(client.is_paused());

        client.unpause(&admin);
        assert!(!client.is_paused());
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_pause_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, _sme) = setup(&env);
        let intruder = Address::generate(&env);
        client.pause(&intruder);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_unpause_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        client.pause(&admin);
        let intruder = Address::generate(&env);
        client.unpause(&intruder);
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_create_invoice_while_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, sme) = setup(&env);
        client.pause(&admin);
        client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_mark_funded_while_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );
        client.pause(&admin);
        client.mark_funded(&id, &pool);
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_mark_paid_while_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );
        client.mark_funded(&id, &pool);
        client.pause(&admin);
        client.mark_paid(&id, &sme);
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_mark_defaulted_while_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );
        client.mark_funded(&id, &pool);
        client.pause(&admin);
        client.mark_defaulted(&id, &pool);
    }

    #[test]
    fn test_views_succeed_while_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );
        client.mark_funded(&id, &pool);
        client.pause(&admin);

        // All views must succeed while paused
        let _ = client.get_invoice(&id);
        let _ = client.get_metadata(&id);
        let _ = client.get_invoice_count();
        let _ = client.get_storage_stats();
        assert!(client.is_paused());
    }

    #[test]
    fn test_pause_unpause_restores_operations() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);

        client.pause(&admin);
        assert!(client.is_paused());
        client.unpause(&admin);
        assert!(!client.is_paused());

        // Should succeed after unpause
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );
        client.mark_funded(&id, &pool);
        client.mark_paid(&id, &sme);
        assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Paid);
    }

    // ---- Issue #56: Pool Address Fix Tests ----

    #[test]
    fn test_pool_address_stored_on_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);
        let hash = String::from_str(&env, "h");

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &hash,
        );

        let invoice = client.get_invoice(&id);
        // pool_contract must be the pool address, never the admin address
        assert_eq!(invoice.pool_contract, pool);
        assert_ne!(invoice.pool_contract, admin);
    }

    #[test]
    #[should_panic(expected = "unauthorized pool")]
    fn test_admin_cannot_bypass_pool_to_mark_funded() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, sme) = setup(&env);
        let hash = String::from_str(&env, "h");

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &hash,
        );
        // Admin must not be able to call mark_funded; only the authorized pool address can
        client.mark_funded(&id, &admin);
    }

    // ---- Issue #61: Edge-Case Tests ----

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_create_invoice_negative_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);

        client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &-1i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );
    }

    #[test]
    fn test_create_invoice_boundary_amount_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &i128::MAX,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );
        assert_eq!(client.get_invoice(&id).amount, i128::MAX);
    }

    #[test]
    #[should_panic(expected = "unauthorized oracle")]
    fn test_verify_invoice_unauthorized_oracle_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, sme) = setup(&env);

        let oracle = Address::generate(&env);
        client.set_oracle(&admin, &oracle);

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );

        let rogue = Address::generate(&env);
        client.verify_invoice(&id, &rogue, &true, &String::from_str(&env, ""));
    }

    #[test]
    #[should_panic(expected = "invoice is not in fundable state")]
    fn test_fund_non_verified_invoice_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);

        let oracle = Address::generate(&env);
        client.set_oracle(&admin, &oracle);

        // Invoice will be in AwaitingVerification after create
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );
        // Trying to fund before verification must panic
        client.mark_funded(&id, &pool);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_mark_paid_random_caller_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, pool, sme) = setup(&env);
        let hash = String::from_str(&env, "h");

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &hash,
        );
        client.mark_funded(&id, &pool);
        let random = Address::generate(&env);
        client.mark_paid(&id, &random);
    }

    #[test]
    #[should_panic(expected = "can only cleanup completed invoices")]
    fn test_cleanup_active_invoice_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, sme) = setup(&env);
        let hash = String::from_str(&env, "h");

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &hash,
        );
        // Invoice is still Pending — cleanup must panic
        client.cleanup_invoice(&id, &admin);
    }

    #[test]
    fn test_daily_invoice_limit_enforced() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, _admin, _pool, sme) = setup(&env);

        let due = env.ledger().timestamp() + 50_000;
        let d = String::from_str(&env, "D");
        let desc = String::from_str(&env, "i");

        // Create exactly MAX_INVOICES_PER_DAY (10) invoices — should all succeed
        for _ in 0..10 {
            client.create_invoice(&sme, &d, &100i128, &due, &desc, &String::from_str(&env, "h"));
        }
    }

    #[test]
    #[should_panic(expected = "daily invoice limit exceeded")]
    fn test_daily_invoice_limit_exceeded_panics() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, _admin, _pool, sme) = setup(&env);

        let due = env.ledger().timestamp() + 50_000;
        let d = String::from_str(&env, "D");
        let desc = String::from_str(&env, "i");

        // Create 11 invoices — the 11th must panic
        for _ in 0..11 {
            client.create_invoice(&sme, &d, &100i128, &due, &desc, &String::from_str(&env, "h"));
        }
    }

    #[test]
    fn test_daily_limit_resets_after_24_hours() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, _admin, _pool, sme) = setup(&env);

        let d = String::from_str(&env, "D");
        let desc = String::from_str(&env, "i");

        // Exhaust today's limit
        for _ in 0..10 {
            let due = env.ledger().timestamp() + 50_000;
            client.create_invoice(&sme, &d, &100i128, &due, &desc, &String::from_str(&env, "h"));
        }

        // Advance 24h+1s
        env.ledger().with_mut(|l| l.timestamp += 86_401);

        // Should succeed again after reset
        let due = env.ledger().timestamp() + 50_000;
        let id = client.create_invoice(&sme, &d, &100i128, &due, &desc, &String::from_str(&env, "h"));
        assert!(id > 0);
    }
}
