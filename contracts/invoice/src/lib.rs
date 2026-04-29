#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, BytesN, Env,
    String, Symbol, Vec,
};

use soroban_sdk::contractclient;

#[contractclient(name = "PoolClient")]
pub trait PoolContract {
    fn is_invoice_repaid(env: Env, invoice_id: u64) -> bool;
}

const LEDGERS_PER_DAY: u32 = 17_280;
const ACTIVE_INVOICE_TTL: u32 = LEDGERS_PER_DAY * 365;
const COMPLETED_INVOICE_TTL: u32 = LEDGERS_PER_DAY * 30;
const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 30;
const INSTANCE_LIFETIME_THRESHOLD: u32 = LEDGERS_PER_DAY * 7;
const UPGRADE_TIMELOCK_SECS: u64 = 86400; // 24 hours
const MAX_INVOICES_PER_DAY: u32 = 10;
const MAX_DAILY_INVOICE_LIMIT: u32 = 1_000;
const SECS_PER_DAY: u64 = 86400;
const DEFAULT_GRACE_PERIOD_DAYS: u32 = 7; // 7 days default grace period
const MAX_GRACE_PERIOD_OVERRIDE_DAYS: u32 = 30; // per-invoice cap (#230)
const DEFAULT_EXPIRATION_DURATION_SECS: u64 = SECS_PER_DAY * 30; // 30 days
const DEFAULT_DISPUTE_RESOLUTION_WINDOW: u64 = SECS_PER_DAY * 30; // 30 days
const MAX_METADATA_URI_LEN: u32 = 256;

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
    Cancelled,
    Expired,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum InvoiceError {
    Unauthorized = 1,
    InvalidStatusTransition = 2,
    InvoiceNotFound = 3,
    HashMismatch = 4,
}

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum DisputeResolution {
    InFavorOfSME,
    InFavorOfDebtor,
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
    pub metadata_uri: Option<String>,
    pub oracle_verified: bool,
    pub dispute_reason: String,
    /// Timestamp when the invoice entered `Disputed` status, or 0 if never disputed.
    pub disputed_at: u64,
    /// Per-invoice grace-period override set by admin (#230).
    /// `None` means use the global `GracePeriodDays` setting.
    /// When set, the value must be ≤ `MAX_GRACE_PERIOD_OVERRIDE_DAYS`.
    pub grace_period_override: Option<u32>,
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

// ── Version tracking (#237) ───────────────────────────────────────────────────

/// Semantic version of the deployed contract, embedded at compile time from
/// `Cargo.toml` via `CARGO_PKG_VERSION`.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ContractVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

/// Parse the `major.minor.patch` version string baked in by Cargo.
fn parse_version() -> ContractVersion {
    let v = env!("CARGO_PKG_VERSION");
    let mut parts = v.splitn(3, '.');
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|s| s.split('-').next()) // strip pre-release suffix
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    ContractVersion {
        major,
        minor,
        patch,
    }
}

/// Current migration level — bump when a schema migration runs.
const CURRENT_MIGRATION_VERSION: u32 = 1;

// ── Debtor registry (#241) ────────────────────────────────────────────────────

/// On-chain record for a verified debtor.
#[contracttype]
#[derive(Clone, Debug)]
pub struct DebtorRecord {
    /// Unique identifier (e.g. company registration number).
    pub debtor_id: String,
    /// Human-readable display name.
    pub debtor_name: String,
    /// Maximum total outstanding invoices permitted for this debtor (in stroops).
    pub max_exposure: i128,
    /// Sum of amounts for all currently-funded invoices to this debtor.
    pub current_exposure: i128,
    pub is_active: bool,
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
    MaxInvoiceAmount,
    ExpirationDurationSecs,
    DailyInvoiceLimit,
    DisputeResolutionWindow,
    /// Stores the [`ContractVersion`] set during `initialize()` (#237).
    ContractVersion,
    /// Incremented each time a migration runs (#237).
    MigrationVersion,
    /// Whether only registered debtors may be invoiced (#241).
    RequireRegisteredDebtor,
    /// Per-debtor record keyed by debtor_id string (#241).
    DebtorRecord(String),
    /// List of all registered debtor IDs (#241).
    DebtorIds,
}

const EVT: Symbol = symbol_short!("INVOICE");

fn maybe_expire_pending_invoice(env: &Env, mut invoice: Invoice) -> Invoice {
    if invoice.status != InvoiceStatus::Pending {
        return invoice;
    }

    let expiration_duration_secs: u64 = env
        .storage()
        .instance()
        .get(&DataKey::ExpirationDurationSecs)
        .unwrap_or(DEFAULT_EXPIRATION_DURATION_SECS);

    let now = env.ledger().timestamp();
    if now <= invoice.created_at.saturating_add(expiration_duration_secs) {
        return invoice;
    }

    invoice.status = InvoiceStatus::Expired;

    env.storage()
        .persistent()
        .set(&DataKey::Invoice(invoice.id), &invoice);
    set_invoice_ttl(env, invoice.id, true);

    let mut stats: StorageStats = env
        .storage()
        .instance()
        .get(&DataKey::StorageStats)
        .unwrap_or_default();
    stats.active_invoices = stats.active_invoices.saturating_sub(1);
    env.storage().instance().set(&DataKey::StorageStats, &stats);

    env.events()
        .publish((EVT, symbol_short!("expired")), invoice.id);

    invoice
}

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

fn is_valid_metadata_uri(env: &Env, uri: &String) -> bool {
    let _ = env;
    let len = uri.len();
    len > 0 && len <= MAX_METADATA_URI_LEN
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

fn invoice_ttl_ledger_expiry(env: &Env, is_completed: bool) -> u64 {
    let ttl = if is_completed {
        COMPLETED_INVOICE_TTL as u64
    } else {
        ACTIVE_INVOICE_TTL as u64
    };
    u64::from(env.ledger().sequence()).saturating_add(ttl)
}

fn is_terminal_invoice_status(status: &InvoiceStatus) -> bool {
    matches!(
        status,
        InvoiceStatus::Paid
            | InvoiceStatus::Defaulted
            | InvoiceStatus::Cancelled
            | InvoiceStatus::Expired
    )
}

fn renew_invoice_ttl(env: &Env, id: u64, status: &InvoiceStatus) -> u64 {
    let is_completed = is_terminal_invoice_status(status);
    set_invoice_ttl(env, id, is_completed);
    invoice_ttl_ledger_expiry(env, is_completed)
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

fn load_invoice(env: &Env, id: u64) -> Invoice {
    env.storage()
        .persistent()
        .get(&DataKey::Invoice(id))
        .expect("invoice not found")
}

#[contract]
pub struct InvoiceContract;

#[contractimpl]
impl InvoiceContract {
    /// Initialize the invoice contract (#245).
    ///
    /// Must be called exactly once after deployment. Subsequent calls panic with
    /// `"already initialized"`.
    ///
    /// # Arguments
    /// * `admin` — Address with administrative control over the contract.
    /// * `pool` — Pool contract address authorized to call `mark_funded`, `mark_paid`,
    ///   and `mark_defaulted`.
    /// * `grace_period_days` — Global default grace period (in days) before a funded
    ///   invoice transitions to `Defaulted`.
    /// * `max_invoice_amount` — Maximum allowed invoice amount in stroops.
    /// * `expiration_duration_secs` — Seconds after creation before a `Pending` invoice
    ///   is auto-expired.
    pub fn initialize(
        env: Env,
        admin: Address,
        pool: Address,
        max_invoice_amount: i128,
        expiration_duration_secs: u64,
        grace_period_days: u32, // new param
    ) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic!("already initialized");
        }
        if max_invoice_amount <= 0 {
            panic!("max invoice amount must be positive");
        }
        if expiration_duration_secs == 0 {
            panic!("expiration duration must be non-zero");
        }
        if grace_period_days > 90 {
            panic!("grace period cannot exceed 90 days");
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
            .set(&DataKey::GracePeriodDays, &grace_period_days); // use param
        env.storage()
            .instance()
            .set(&DataKey::MaxInvoiceAmount, &max_invoice_amount);
        env.storage()
            .instance()
            .set(&DataKey::ExpirationDurationSecs, &expiration_duration_secs);
        env.storage().instance().set(
            &DataKey::DisputeResolutionWindow,
            &DEFAULT_DISPUTE_RESOLUTION_WINDOW,
        );
        // Store compile-time version (#237)
        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &parse_version());
        env.storage()
            .instance()
            .set(&DataKey::MigrationVersion, &0u32);
        // Debtor registry defaults (#241)
        env.storage()
            .instance()
            .set(&DataKey::RequireRegisteredDebtor, &false);
        env.storage()
            .instance()
            .set(&DataKey::DebtorIds, &Vec::<String>::new(&env));
        bump_instance(&env);
    }

    /// Returns the semantic version of this deployed contract (#237).
    pub fn version(env: Env) -> ContractVersion {
        env.storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap_or_else(parse_version)
    }

    /// Returns the current migration version (#237).
    pub fn migration_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MigrationVersion)
            .unwrap_or(0)
    }

    /// Run pending migrations up to `CURRENT_MIGRATION_VERSION` (#237).
    /// Idempotent — safe to call multiple times.
    pub fn run_migration(env: Env, admin: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized");
        }
        let current: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MigrationVersion)
            .unwrap_or(0);
        if current >= CURRENT_MIGRATION_VERSION {
            return;
        }
        // Future migration steps go here, guarded by version checks.
        env.storage()
            .instance()
            .set(&DataKey::MigrationVersion, &CURRENT_MIGRATION_VERSION);
    }

    // ── Debtor registry (#241) ────────────────────────────────────────────────

    /// Register a debtor in the on-chain whitelist.
    ///
    /// Only the admin may call this. `max_exposure` is the maximum total
    /// outstanding funded-invoice amount permitted for this debtor (in stroops).
    pub fn register_debtor(
        env: Env,
        admin: Address,
        debtor_id: String,
        debtor_name: String,
        max_exposure: i128,
    ) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized");
        }
        if max_exposure <= 0 {
            panic!("max_exposure must be positive");
        }
        let record = DebtorRecord {
            debtor_id: debtor_id.clone(),
            debtor_name,
            max_exposure,
            current_exposure: 0,
            is_active: true,
        };
        env.storage()
            .persistent()
            .set(&DataKey::DebtorRecord(debtor_id.clone()), &record);
        // Append to ID list if not already present
        let mut ids: Vec<String> = env
            .storage()
            .instance()
            .get(&DataKey::DebtorIds)
            .unwrap_or_else(|| Vec::new(&env));
        if !ids.contains(&debtor_id) {
            ids.push_back(debtor_id);
            env.storage().instance().set(&DataKey::DebtorIds, &ids);
        }
    }

    /// Deactivate a registered debtor. Existing invoices are unaffected.
    pub fn deactivate_debtor(env: Env, admin: Address, debtor_id: String) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized");
        }
        let mut record: DebtorRecord = env
            .storage()
            .persistent()
            .get(&DataKey::DebtorRecord(debtor_id.clone()))
            .expect("debtor not found");
        record.is_active = false;
        env.storage()
            .persistent()
            .set(&DataKey::DebtorRecord(debtor_id), &record);
    }

    /// Fetch a debtor record by ID. Panics if not registered.
    pub fn get_debtor(env: Env, debtor_id: String) -> DebtorRecord {
        env.storage()
            .persistent()
            .get(&DataKey::DebtorRecord(debtor_id))
            .expect("debtor not found")
    }

    /// Return all registered debtor IDs.
    pub fn list_debtors(env: Env) -> Vec<String> {
        env.storage()
            .instance()
            .get(&DataKey::DebtorIds)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Toggle the require-registered-debtor flag.
    /// When `true`, `create_invoice` rejects invoices for unregistered debtors.
    pub fn set_require_registered_debtor(env: Env, admin: Address, required: bool) {
        admin.require_auth();
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
            .set(&DataKey::RequireRegisteredDebtor, &required);
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

    /// Create a new invoice on behalf of the calling SME (#245).
    ///
    /// # Arguments
    /// * `owner` — SME address that owns the invoice; must sign the call.
    /// * `debtor` — Debtor identifier string (or registered `debtor_id` when
    ///   `require_registered_debtor` is `true`).
    /// * `amount` — Invoice value in token stroops (must be ≤ `max_invoice_amount`).
    /// * `due_date` — Unix timestamp by which repayment is expected (must be future).
    /// * `description` — Human-readable invoice description.
    /// * `verification_hash` — Off-chain document hash for oracle verification.
    ///
    /// # Returns
    /// The newly assigned invoice ID (monotonically increasing from 1).
    ///
    /// # Errors
    /// Panics if the contract is paused, amount is non-positive, amount exceeds
    /// the configured maximum, due date is not in the future, the daily rate limit
    /// is exceeded, or the debtor fails registry validation (when enabled).
    pub fn create_invoice(
        env: Env,
        owner: Address,
        debtor: String,
        amount: i128,
        due_date: u64,
        description: String,
        verification_hash: String,
    ) -> u64 {
        Self::create_invoice_with_metadata(
            env,
            owner,
            debtor,
            amount,
            due_date,
            description,
            verification_hash,
            None,
        )
    }

    pub fn create_invoice_with_metadata(
        env: Env,
        owner: Address,
        debtor: String,
        amount: i128,
        due_date: u64,
        description: String,
        verification_hash: String,
        metadata_uri: Option<String>,
    ) -> u64 {
        owner.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        if amount <= 0 {
            panic!("amount must be positive");
        }
        let max_invoice_amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MaxInvoiceAmount)
            .expect("max invoice amount not set");
        if amount > max_invoice_amount {
            panic!("invoice amount exceeds maximum");
        }
        if due_date <= env.ledger().timestamp() {
            panic!("due date must be in the future");
        }

        // Debtor registry validation (#241)
        let require_registered: bool = env
            .storage()
            .instance()
            .get(&DataKey::RequireRegisteredDebtor)
            .unwrap_or(false);
        if require_registered {
            let mut record: DebtorRecord = env
                .storage()
                .persistent()
                .get(&DataKey::DebtorRecord(debtor.clone()))
                .expect("debtor not registered");
            if !record.is_active {
                panic!("debtor is not active");
            }
            if record.current_exposure + amount > record.max_exposure {
                panic!("invoice would exceed debtor exposure limit");
            }
            record.current_exposure += amount;
            env.storage()
                .persistent()
                .set(&DataKey::DebtorRecord(debtor.clone()), &record);
        }

        // Rate limiting: use the configured daily limit when present, otherwise
        // fall back to the historical default of 10 invoices per address.
        let daily_limit: u32 = env
            .storage()
            .instance()
            .get(&DataKey::DailyInvoiceLimit)
            .unwrap_or(MAX_INVOICES_PER_DAY);
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

        if daily_count >= daily_limit {
            panic!("daily invoice limit exceeded");
        }

        daily_count += 1;
        env.storage().instance().set(&daily_count_key, &daily_count);

        if let Some(uri) = metadata_uri.clone() {
            if !is_valid_metadata_uri(&env, &uri) {
                panic!("invalid metadata uri");
            }
        }

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
            metadata_uri: metadata_uri.clone(),
            oracle_verified: false,
            dispute_reason: empty_str,
            disputed_at: 0,
            grace_period_override: None,
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

        env.events().publish(
            (EVT, symbol_short!("created")),
            (id, owner, amount, metadata_uri, env.ledger().timestamp()),
        );

        id
    }

    pub fn set_daily_invoice_limit(env: Env, admin: Address, limit: u32) {
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
        if limit == 0 {
            panic!("daily invoice limit must be positive");
        }
        if limit > MAX_DAILY_INVOICE_LIMIT {
            panic!("daily invoice limit too high");
        }

        env.storage()
            .instance()
            .set(&DataKey::DailyInvoiceLimit, &limit);
        env.events()
            .publish((EVT, symbol_short!("set_limit")), (admin, limit));
    }

    pub fn get_daily_invoice_limit(env: Env) -> u32 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::DailyInvoiceLimit)
            .unwrap_or(MAX_INVOICES_PER_DAY)
    }

    pub fn verify_invoice(
        env: Env,
        id: u64,
        oracle: Address,
        approved: bool,
        reason: String,
        oracle_hash: String,
    ) -> Result<(), InvoiceError> {
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

        if invoice.verification_hash != oracle_hash {
            return Err(InvoiceError::HashMismatch);
        }

        if approved {
            invoice.status = InvoiceStatus::Verified;
            invoice.oracle_verified = true;
        } else {
            invoice.status = InvoiceStatus::Disputed;
            invoice.dispute_reason = reason;
            invoice.disputed_at = env.ledger().timestamp();
        }

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);
        set_invoice_ttl(&env, id, false);

        if approved {
            env.events()
                .publish((EVT, symbol_short!("verified")), (id, oracle_hash));
        } else {
            env.events().publish((EVT, symbol_short!("disputed")), (id, env.ledger().timestamp()));
        }
        Ok(())
    }

    pub fn resolve_dispute(env: Env, id: u64, caller: Address, resolution: DisputeResolution) {
        caller.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");

        if invoice.status != InvoiceStatus::Disputed {
            panic!("invoice is not disputed");
        }

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        let oracle: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .expect("oracle not configured");

        if caller == oracle {
            // Oracle can always resolve
        } else if caller == admin {
            // Admin can resolve only after window
            let window: u64 = env
                .storage()
                .instance()
                .get(&DataKey::DisputeResolutionWindow)
                .unwrap_or(DEFAULT_DISPUTE_RESOLUTION_WINDOW);
            if env.ledger().timestamp() < invoice.disputed_at.saturating_add(window) {
                panic!("dispute resolution window not yet passed for admin");
            }
        } else {
            panic!("unauthorized");
        }

        match resolution {
            DisputeResolution::InFavorOfSME => {
                invoice.status = InvoiceStatus::Verified;
                invoice.oracle_verified = true;
                invoice.dispute_reason = String::from_str(&env, "");
            }
            DisputeResolution::InFavorOfDebtor => {
                invoice.status = InvoiceStatus::Cancelled;
                let mut stats: StorageStats = env
                    .storage()
                    .instance()
                    .get(&DataKey::StorageStats)
                    .unwrap_or_default();
                stats.active_invoices = stats.active_invoices.saturating_sub(1);
                env.storage().instance().set(&DataKey::StorageStats, &stats);
                set_invoice_ttl(&env, id, true);
            }
        }

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);

        env.events()
            .publish((EVT, symbol_short!("resolved")), (id, resolution, caller));
    }

    pub fn set_dispute_window(env: Env, admin: Address, window: u64) {
        admin.require_auth();
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
            .set(&DataKey::DisputeResolutionWindow, &window);
        bump_instance(&env);
    }

    pub fn get_dispute_window(env: Env) -> u64 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::DisputeResolutionWindow)
            .unwrap_or(DEFAULT_DISPUTE_RESOLUTION_WINDOW)
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

        invoice = maybe_expire_pending_invoice(&env, invoice);
        if invoice.status == InvoiceStatus::Expired {
            panic!("invoice is expired");
        }

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
        env.events().publish((EVT, symbol_short!("funded")), (id, env.ledger().timestamp()));
    }

    pub fn mark_paid(env: Env, id: u64, pool: Address) {
        pool.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let authorized_pool: Address = env
            .storage()
            .instance()
            .get(&DataKey::Pool)
            .expect("not initialized");
        if pool != authorized_pool {
            panic!("unauthorized: only pool can mark paid");
        }

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");

        if invoice.status != InvoiceStatus::Funded {
            panic!("invoice is not funded");
        }

        // CRITICAL: Verify with pool that repayment actually occurred
        let pool_client = PoolClient::new(&env, &pool);
        if !pool_client.is_invoice_repaid(&id) {
            panic!("repayment not verified by pool contract");
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

        env.events().publish((EVT, symbol_short!("paid")), (id, env.ledger().timestamp()));
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

        // Use the invoice's per-invoice override if set; fall back to global (#230).
        let global_grace: u32 = env
            .storage()
            .instance()
            .get(&DataKey::GracePeriodDays)
            .unwrap_or(DEFAULT_GRACE_PERIOD_DAYS);
        let grace_period_days = invoice.grace_period_override.unwrap_or(global_grace);
        let grace_period_secs = grace_period_days as u64 * SECS_PER_DAY;
        let now = env.ledger().timestamp();
        let default_at = invoice.due_date + grace_period_secs;

        if now < default_at {
            panic!(
                "grace period has not elapsed: default available at {}",
                default_at
            );
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

        env.events().publish((EVT, symbol_short!("default")), (id, env.ledger().timestamp()));
    }

    pub fn cancel_invoice(env: Env, id: u64, caller: Address) {
        caller.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");

        invoice = maybe_expire_pending_invoice(&env, invoice);

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");

        let can_cancel = if caller == invoice.owner {
            matches!(
                invoice.status,
                InvoiceStatus::Pending
                    | InvoiceStatus::AwaitingVerification
                    | InvoiceStatus::Verified
            )
        } else if caller == admin {
            matches!(
                invoice.status,
                InvoiceStatus::Pending
                    | InvoiceStatus::AwaitingVerification
                    | InvoiceStatus::Verified
                    | InvoiceStatus::Disputed
            )
        } else {
            false
        };

        if !can_cancel {
            if caller != invoice.owner && caller != admin {
                panic!("unauthorized");
            }
            panic!("invalid status transition");
        }

        invoice.status = InvoiceStatus::Cancelled;

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

        env.events()
            .publish((EVT, symbol_short!("cancelled")), (id, caller));
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

        let is_completed = invoice.status == InvoiceStatus::Paid
            || invoice.status == InvoiceStatus::Defaulted
            || invoice.status == InvoiceStatus::Cancelled
            || invoice.status == InvoiceStatus::Expired;
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

    /// Renew the storage TTL for an invoice.
    ///
    /// Active invoices receive the longer renewal window, while terminal
    /// invoices keep the short post-settlement retention period.
    pub fn renew_ttl(env: Env, id: u64) -> u64 {
        bump_instance(&env);
        let invoice = load_invoice(&env, id);
        let new_expiry_ledger = renew_invoice_ttl(&env, id, &invoice.status);
        env.events()
            .publish((EVT, symbol_short!("ttl_ren")), (id, new_expiry_ledger));
        new_expiry_ledger
    }

    /// Renew TTLs for multiple invoices in a single call.
    pub fn batch_renew_ttl(env: Env, ids: Vec<u64>) -> Vec<u64> {
        bump_instance(&env);
        if ids.len() > 20 {
            panic!("batch_renew_ttl: max 20 IDs per call");
        }

        let mut renewed: Vec<u64> = Vec::new(&env);
        for i in 0..ids.len() {
            let id = ids.get(i).unwrap();
            let invoice = load_invoice(&env, id);
            let expiry = renew_invoice_ttl(&env, id, &invoice.status);
            env.events()
                .publish((EVT, symbol_short!("ttl_ren")), (id, expiry));
            renewed.push_back(expiry);
        }
        renewed
    }

    pub fn get_invoice(env: Env, id: u64) -> Invoice {
        bump_instance(&env);
        let inv = load_invoice(&env, id);
        maybe_expire_pending_invoice(&env, inv)
    }

    pub fn get_multiple_invoices(env: Env, ids: Vec<u64>) -> Vec<Invoice> {
        bump_instance(&env);
        let mut invoices: Vec<Invoice> = Vec::new(&env);
        for i in 0..ids.len() {
            let inv = load_invoice(&env, ids.get(i).unwrap());
            invoices.push_back(maybe_expire_pending_invoice(&env, inv));
        }
        invoices
    }

    /// SEP-oriented metadata for invoice id `id` (same ledger fields as `get_invoice`).
    pub fn get_metadata(env: Env, id: u64) -> InvoiceMetadata {
        let inv = load_invoice(&env, id);
        let inv = maybe_expire_pending_invoice(&env, inv);

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

    /// Check if a pending invoice has expired and transition it to `Expired` status.
    /// Returns `true` if the invoice was expired, `false` otherwise.
    /// Emits `(INVOICE, "expired")` event if expired.
    pub fn check_expiration(env: Env, id: u64) -> bool {
        bump_instance(&env);
        let inv = load_invoice(&env, id);

        if inv.status != InvoiceStatus::Pending {
            return false;
        }

        let expiration_duration_secs: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ExpirationDurationSecs)
            .unwrap_or(DEFAULT_EXPIRATION_DURATION_SECS);

        let now = env.ledger().timestamp();
        if now <= inv.created_at.saturating_add(expiration_duration_secs) {
            return false;
        }

        let mut expired_inv = inv;
        expired_inv.status = InvoiceStatus::Expired;
        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &expired_inv);
        set_invoice_ttl(&env, id, true);

        let mut stats: StorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        stats.active_invoices = stats.active_invoices.saturating_sub(1);
        env.storage().instance().set(&DataKey::StorageStats, &stats);

        env.events().publish((EVT, symbol_short!("expired")), id);

        true
    }

    /// Batch check expiration for multiple invoices (up to 20 per call for safety).
    /// Returns the count of newly expired invoices.
    pub fn batch_check_expiration(env: Env, ids: Vec<u64>) -> u32 {
        bump_instance(&env);

        let batch_size = ids.len();
        if batch_size > 20 {
            panic!("batch_check_expiration: max 20 IDs per call");
        }

        let mut expired_count = 0u32;
        for i in 0..batch_size {
            let id = ids.get(i).unwrap();
            if Self::check_expiration(env.clone(), id) {
                expired_count += 1;
            }
        }

        expired_count
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

    pub fn set_max_invoice_amount(env: Env, admin: Address, max_invoice_amount: i128) {
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
        if max_invoice_amount <= 0 {
            panic!("max invoice amount must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxInvoiceAmount, &max_invoice_amount);
        env.events()
            .publish((EVT, symbol_short!("set_max")), (admin, max_invoice_amount));
    }

    pub fn get_max_invoice_amount(env: Env) -> i128 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::MaxInvoiceAmount)
            .expect("max invoice amount not set")
    }

    pub fn set_expiration_duration(env: Env, admin: Address, expiration_duration_secs: u64) {
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
        if expiration_duration_secs == 0 {
            panic!("expiration duration must be non-zero");
        }
        env.storage()
            .instance()
            .set(&DataKey::ExpirationDurationSecs, &expiration_duration_secs);
        env.events().publish(
            (EVT, symbol_short!("set_exp")),
            (admin, expiration_duration_secs),
        );
    }

    pub fn get_expiration_duration(env: Env) -> u64 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::ExpirationDurationSecs)
            .unwrap_or(DEFAULT_EXPIRATION_DURATION_SECS)
    }

    pub fn get_grace_period(env: Env) -> u32 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::GracePeriodDays)
            .unwrap_or(DEFAULT_GRACE_PERIOD_DAYS)
    }

    /// Set a per-invoice grace-period override (#230).
    ///
    /// Only callable by admin on a `Funded` invoice.
    /// `days` must be ≤ `MAX_GRACE_PERIOD_OVERRIDE_DAYS` (30).
    /// Emits `(INVOICE, "grace_period_updated")` with `(id, old_days, new_days)`.
    pub fn set_invoice_grace_period(env: Env, admin: Address, id: u64, days: u32) {
        admin.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized: caller is not admin");
        }

        if days > MAX_GRACE_PERIOD_OVERRIDE_DAYS {
            panic!(
                "grace period override {} days exceeds maximum of {} days",
                days, MAX_GRACE_PERIOD_OVERRIDE_DAYS
            );
        }

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");

        if invoice.status != InvoiceStatus::Funded {
            panic!("grace period override only allowed on Funded invoices");
        }

        let global_grace: u32 = env
            .storage()
            .instance()
            .get(&DataKey::GracePeriodDays)
            .unwrap_or(DEFAULT_GRACE_PERIOD_DAYS);
        let old_days = invoice.grace_period_override.unwrap_or(global_grace);

        invoice.grace_period_override = Some(days);
        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);

        env.events()
            .publish((EVT, symbol_short!("gp_upd")), (id, old_days, days));
    }

    /// Get the effective grace period for a specific invoice (#230).
    /// Returns the per-invoice override if set, otherwise the global default.
    pub fn get_invoice_grace_period(env: Env, id: u64) -> u32 {
        bump_instance(&env);
        let invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");
        let global_grace: u32 = env
            .storage()
            .instance()
            .get(&DataKey::GracePeriodDays)
            .unwrap_or(DEFAULT_GRACE_PERIOD_DAYS);
        invoice.grace_period_override.unwrap_or(global_grace)
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
        env.deployer().update_current_contract_wasm(wasm_hash);
        env.events()
            .publish((EVT, symbol_short!("upgraded")), (admin, now));
    }

    pub fn check_default_warning(env: Env, id: u64) -> bool {
        let invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");

        if invoice.status != InvoiceStatus::Funded {
            return false;
        }

        let grace_period_days: u32 = env
            .storage()
            .instance()
            .get(&DataKey::GracePeriodDays)
            .unwrap_or(DEFAULT_GRACE_PERIOD_DAYS);
        let default_at = invoice.due_date + grace_period_days as u64 * SECS_PER_DAY;
        let now = env.ledger().timestamp();

        // Within 24h of default
        if now >= invoice.due_date && now < default_at && default_at - now <= SECS_PER_DAY {
            env.events()
                .publish((EVT, symbol_short!("def_warn")), (id, default_at));
            return true;
        }
        false
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
        client.initialize(
            &admin,
            &pool,
            &i128::MAX,
            &DEFAULT_EXPIRATION_DURATION_SECS,
            &u32::MAX,
        );
        (client, admin, pool, sme)
    }

    fn setup_with_oracle(
        env: &Env,
    ) -> (
        InvoiceContractClient<'_>,
        Address,
        Address,
        Address,
        Address,
    ) {
        let (client, admin, pool, sme) = setup(env);
        let oracle = Address::generate(env);
        client.set_oracle(&admin, &oracle);
        (client, admin, pool, sme, oracle)
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

        client
            .verify_invoice(&id, &oracle, &true, &String::from_str(&env, ""), &hash)
            .unwrap();
        let invoice = client.get_invoice(&id);
        assert_eq!(invoice.status, InvoiceStatus::Verified);
        assert!(invoice.oracle_verified);

        client.mark_funded(&id, &pool);
        assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Funded);
    }

    #[test]
    fn test_verify_invoice_hash_mismatch() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, sme) = setup(&env);

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

        let result = client.try_verify_invoice(
            &id,
            &oracle,
            &true,
            &String::from_str(&env, ""),
            &String::from_str(&env, "wrong_hash"),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_verify_invoice_zero_hash_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, sme) = setup(&env);

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

        let zero_hash = String::from_str(&env, "");
        let result =
            client.try_verify_invoice(&id, &oracle, &true, &String::from_str(&env, ""), &zero_hash);

        assert!(result.is_err());
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
        client
            .verify_invoice(&id, &oracle, &false, &reason, &String::from_str(&env, ""))
            .unwrap();
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

    // ---- Cancellation Tests ----

    #[test]
    fn test_cancel_pending_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );

        let stats_before = client.get_storage_stats();
        assert_eq!(stats_before.active_invoices, 1);

        client.cancel_invoice(&id, &sme);
        let inv = client.get_invoice(&id);
        assert_eq!(inv.status, InvoiceStatus::Cancelled);

        let stats_after = client.get_storage_stats();
        assert_eq!(stats_after.active_invoices, 0);
    }

    #[test]
    #[should_panic(expected = "only pending invoices can be cancelled")]
    fn test_cancel_non_pending_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, pool, sme) = setup(&env);

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );
        client.mark_funded(&id, &pool);
        client.cancel_invoice(&id, &sme);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_cancel_unauthorized_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );

        let intruder = Address::generate(&env);
        client.cancel_invoice(&id, &intruder);
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
        client.verify_invoice(
            &id,
            &rogue,
            &true,
            &String::from_str(&env, ""),
            &String::from_str(&env, ""),
        );
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
            client.create_invoice(
                &sme,
                &d,
                &100i128,
                &due,
                &desc,
                &String::from_str(&env, "h"),
            );
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
            client.create_invoice(
                &sme,
                &d,
                &100i128,
                &due,
                &desc,
                &String::from_str(&env, "h"),
            );
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
            client.create_invoice(
                &sme,
                &d,
                &100i128,
                &due,
                &desc,
                &String::from_str(&env, "h"),
            );
        }

        // Advance 24h+1s
        env.ledger().with_mut(|l| l.timestamp += 86_401);

        // Should succeed again after reset
        let due = env.ledger().timestamp() + 50_000;
        let id = client.create_invoice(
            &sme,
            &d,
            &100i128,
            &due,
            &desc,
            &String::from_str(&env, "h"),
        );
        assert!(id > 0);
    }

    #[test]
    fn test_daily_invoice_limit_defaults_to_ten() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, _sme) = setup(&env);

        assert_eq!(client.get_daily_invoice_limit(), 10);
    }

    #[test]
    fn test_admin_can_set_daily_invoice_limit() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _pool, sme) = setup(&env);

        client.set_daily_invoice_limit(&admin, &20u32);
        assert_eq!(client.get_daily_invoice_limit(), 20);

        let due = env.ledger().timestamp() + 50_000;
        let d = String::from_str(&env, "D");
        let desc = String::from_str(&env, "i");
        let hash = String::from_str(&env, "h");

        for _ in 0..20 {
            client.create_invoice(&sme, &d, &100i128, &due, &desc, &hash);
        }
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_set_daily_invoice_limit_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, _sme) = setup(&env);
        let intruder = Address::generate(&env);

        client.set_daily_invoice_limit(&intruder, &20u32);
    }

    #[test]
    #[should_panic(expected = "daily invoice limit must be positive")]
    fn test_set_daily_invoice_limit_zero_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);

        client.set_daily_invoice_limit(&admin, &0u32);
    }

    #[test]
    #[should_panic(expected = "daily invoice limit too high")]
    fn test_set_daily_invoice_limit_above_max_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);

        client.set_daily_invoice_limit(&admin, &1_001u32);
    }

    // ---- Max Invoice Amount Tests ----

    #[test]
    #[should_panic(expected = "invoice amount exceeds maximum")]
    fn test_create_invoice_exceeds_max_panics() {
        let env = Env::default();
        env.mock_all_auths();

        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let pool = Address::generate(&env);
        let sme = Address::generate(&env);

        client.initialize(
            &admin,
            &pool,
            &1_000i128,
            &DEFAULT_EXPIRATION_DURATION_SECS,
            &u32::MAX,
        );

        client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_001i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );
    }

    #[test]
    fn test_admin_can_update_max_invoice_amount() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);

        assert_eq!(client.get_max_invoice_amount(), i128::MAX);
        client.set_max_invoice_amount(&admin, &123_456i128);
        assert_eq!(client.get_max_invoice_amount(), 123_456i128);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_set_max_invoice_amount_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, _sme) = setup(&env);

        let intruder = Address::generate(&env);
        client.set_max_invoice_amount(&intruder, &100i128);
    }

    // ---- Expiration Tests ----

    #[test]
    fn test_pending_invoice_expires_after_duration_on_read() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let pool = Address::generate(&env);
        let sme = Address::generate(&env);

        client.initialize(&admin, &pool, &i128::MAX, &10u64, &u32::MAX);

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );

        env.ledger().with_mut(|l| l.timestamp += 11);

        let inv = client.get_invoice(&id);
        assert_eq!(inv.status, InvoiceStatus::Expired);
    }

    #[test]
    #[should_panic(expected = "invoice is expired")]
    fn test_expired_invoice_cannot_be_funded() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let pool = Address::generate(&env);
        let sme = Address::generate(&env);

        client.initialize(&admin, &pool, &i128::MAX, &1u64, &u32::MAX);

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );

        env.ledger().with_mut(|l| l.timestamp += 2);

        client.mark_funded(&id, &pool);
    }

    #[test]
    #[should_panic(expected = "unauthorized: only pool can mark paid")]
    fn test_mark_paid_by_owner_panics() {
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

        // SME should no longer be able to mark paid
        client.mark_paid(&id, &sme);
    }

    #[test]
    #[should_panic(expected = "unauthorized: only pool can mark paid")]
    fn test_mark_paid_by_admin_panics() {
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

        // Admin should no longer be able to mark paid
        client.mark_paid(&id, &admin);
    }

    #[test]
    #[should_panic(expected = "repayment not verified by pool contract")]
    fn test_mark_paid_without_pool_verification_panics() {
        let env = Env::default();
        env.mock_all_auths();

        // Register mock pool that returns false for is_invoice_repaid
        let pool_id = env.register(MockPoolFalse, ());
        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);

        client.initialize(
            &admin,
            &pool_id,
            &i128::MAX,
            &DEFAULT_EXPIRATION_DURATION_SECS,
            &u32::MAX,
        );
        let hash = String::from_str(&env, "h");

        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 10_000),
            &String::from_str(&env, "x"),
            &hash,
        );
        client.mark_funded(&id, &pool_id);

        // Pool tries to mark paid but repayment not verified -> panic
        client.mark_paid(&id, &pool_id);
    }

    // You'll need a mock pool contract for testing:
    #[contract]
    struct MockPoolFalse;

    #[contractimpl]
    impl MockPoolFalse {
        pub fn is_invoice_repaid(_env: Env, _invoice_id: u64) -> bool {
            false
        }
    }

    #[test]
    #[should_panic(expected = "grace period has not elapsed")]
    fn test_mark_defaulted_during_grace_period_panics() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let pool = Address::generate(&env);
        let sme = Address::generate(&env);

        // 7 day grace period
        client.initialize(
            &admin,
            &pool,
            &i128::MAX,
            &DEFAULT_EXPIRATION_DURATION_SECS,
            &7u32,
        );

        let due_date = 100_000 + 86400; // 1 day from now
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &due_date,
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );
        client.mark_funded(&id, &pool);

        // Move to 2 days past due = still in 7 day grace period
        env.ledger()
            .with_mut(|l| l.timestamp = due_date + 2 * 86400);
        client.mark_defaulted(&id, &pool); // should panic
    }

    #[test]
    fn test_mark_defaulted_after_grace_period_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let pool = Address::generate(&env);
        let sme = Address::generate(&env);

        client.initialize(
            &admin,
            &pool,
            &i128::MAX,
            &DEFAULT_EXPIRATION_DURATION_SECS,
            &7u32,
        );

        let due_date = 100_000 + 86400;
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &due_date,
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
        );
        client.mark_funded(&id, &pool);

        // Move to 8 days past due = grace period elapsed
        env.ledger()
            .with_mut(|l| l.timestamp = due_date + 8 * 86400);
        client.mark_defaulted(&id, &pool);
        assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Defaulted);
    }

    #[test]
    fn test_set_grace_period_updates_value() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup_with_grace(&env, 7);

        client.set_grace_period(&admin, &14u32);
        assert_eq!(client.get_grace_period(), 14u32);
    }

    // helper for tests
    fn setup_with_grace(
        env: &Env,
        grace_days: u32,
    ) -> (InvoiceContractClient<'_>, Address, Address, Address) {
        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let pool = Address::generate(env);
        let sme = Address::generate(env);
        client.initialize(
            &admin,
            &pool,
            &i128::MAX,
            &DEFAULT_EXPIRATION_DURATION_SECS,
            &grace_days,
        );
        (client, admin, pool, sme)
    }

    #[test]
    fn test_pause_blocks_funding_and_payment() {
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
        assert!(client.is_paused());
    }

    // ── #230: per-invoice grace period tests ────────────────────────────────

    #[test]
    fn test_invoice_without_override_uses_global_grace_period() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, _owner) = setup_funded_invoice(&env);

        // No override set — effective grace period should be global (7 days by default).
        let id = 1u64;
        let effective = client.get_invoice_grace_period(&id);
        assert_eq!(effective, DEFAULT_GRACE_PERIOD_DAYS);
        let _ = admin;
        let _ = pool;
    }

    #[test]
    fn test_invoice_with_override_uses_override_grace_period() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _owner) = setup_funded_invoice(&env);

        let id = 1u64;
        client.set_invoice_grace_period(&admin, &id, &14u32);
        let effective = client.get_invoice_grace_period(&id);
        assert_eq!(effective, 14);
    }

    #[test]
    #[should_panic(expected = "exceeds maximum")]
    fn test_override_exceeds_max_days_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _owner) = setup_funded_invoice(&env);

        // 31 days > MAX_GRACE_PERIOD_OVERRIDE_DAYS (30) — must panic.
        client.set_invoice_grace_period(&admin, &1u64, &31u32);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_non_admin_cannot_set_override() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, owner) = setup_funded_invoice(&env);

        // owner is not admin — must panic.
        client.set_invoice_grace_period(&owner, &1u64, &10u32);
    }

    /// Helper: initialise a contract and produce a single funded invoice (id=1).
    fn setup_funded_invoice(env: &Env) -> (InvoiceContractClient<'_>, Address, Address, Address) {
        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(env, &contract_id);

        let admin = Address::generate(env);
        let pool = Address::generate(env);
        let oracle = Address::generate(env);
        let owner = Address::generate(env);

        client.initialize(&admin, &pool, &oracle);

        let id = client.create_invoice(
            &owner,
            &String::from_str(env, "ACME Corp"),
            &1_000_0000000i128,
            &(env.ledger().timestamp() + SECS_PER_DAY * 30),
            &String::from_str(env, "Test invoice"),
            &String::from_str(env, "hash"),
        );

        // Verify and fund so the invoice reaches Funded status.
        client
            .verify_invoice(
                &id,
                &oracle,
                &true,
                &String::from_str(env, ""),
                &String::from_str(env, "hash"),
            )
            .unwrap();
        client.mark_funded(&id, &pool);

        (client, admin, pool, owner)
    }
}
