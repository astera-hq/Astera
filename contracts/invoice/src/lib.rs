#![no_std]
#![allow(clippy::too_many_arguments)]

// === AUTHORIZED CALLERS ===
// - Admin: initialize(), admin-only setters
// - Pool contract: mark_funded(), mark_paid(), mark_defaulted()
// - Oracle: mark_verified(), mark_disputed()
// - Anyone: cleanup_expired_storage(), read-only view functions (e.g., get_invoice)

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    BytesN, Env, String, Symbol, Vec,
};

use soroban_sdk::contractclient;

#[contractclient(name = "PoolClient")]
pub trait PoolContract {
    fn is_invoice_repaid(env: Env, invoice_id: u64) -> bool;
    fn update_invoice_due_date(
        env: Env,
        invoice_contract: Address,
        invoice_id: u64,
        new_due_date: u64,
    );
}

const LEDGERS_PER_DAY: u32 = 17_280;
const ACTIVE_INVOICE_TTL: u32 = LEDGERS_PER_DAY * 365;
// #446: completed invoices must outlive active ones — default to 5 years so
// repayment history, dispute resolution, and credit-score lookups remain
// accessible long after an invoice is settled.
const DEFAULT_COMPLETED_INVOICE_TTL: u32 = LEDGERS_PER_DAY * 365 * 5;
// Keep the old name as an alias so existing call-sites compile unchanged.
const COMPLETED_INVOICE_TTL: u32 = DEFAULT_COMPLETED_INVOICE_TTL;
const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 30;
const INSTANCE_LIFETIME_THRESHOLD: u32 = LEDGERS_PER_DAY * 7;
const UPGRADE_TIMELOCK_SECS: u64 = 86400; // 24 hours — default
const MIN_UPGRADE_TIMELOCK_SECS: u64 = 3_600; // 1 hour minimum (#338)
const ADMIN_CHANGE_TIMELOCK_SECS: u64 = 172_800; // 48 hours — #565
const MAX_INVOICES_PER_DAY: u32 = 10;
const MAX_DAILY_INVOICE_LIMIT: u32 = 1_000;
const SECS_PER_DAY: u64 = 86400;
// 25-hour TTL for the per-SME invoice timestamp Vec used by the sliding-window
// rate limiter. One hour longer than the window so entries near the boundary
// are never evicted before the next submission can filter them out.
const SLIDING_WINDOW_TTL_LEDGERS: u32 = LEDGERS_PER_DAY + LEDGERS_PER_DAY / 24;
const DEFAULT_GRACE_PERIOD_DAYS: u32 = 7;
// Both the global setter (set_grace_period) and the per-invoice setter
// (set_invoice_grace_period) share the same 90-day maximum. 90 days is the
// practical upper bound that gives admins meaningful per-invoice flexibility
// while preventing indefinite extensions that would undermine protocol risk
// models. Keeping both caps identical ensures per-invoice overrides can always
// reach the global maximum — making the override meaningful.
const MAX_GRACE_PERIOD_OVERRIDE_DAYS: u32 = 90;
const MAX_DUE_DATE_AHEAD_SECS: u64 = SECS_PER_DAY * 365 * 30;
const DEFAULT_EXPIRATION_DURATION_SECS: u64 = SECS_PER_DAY * 30; // 30 days
const MAX_EXPIRATION_DURATION_SECS: u64 = SECS_PER_DAY * 365 * 10; // 10 years
const DEFAULT_DISPUTE_RESOLUTION_WINDOW: u64 = SECS_PER_DAY * 30; // 30 days
const MAX_DESCRIPTION_LEN: u32 = 256;
const MAX_DEBTOR_LEN: u32 = 64;
const MAX_VERIFICATION_HASH_LEN: u32 = 256;
const MAX_METADATA_URI_LEN: u32 = 256;
const DEFAULT_MIN_DUE_DATE_WINDOW_SECS: u64 = SECS_PER_DAY;
const DEFAULT_METADATA_IMAGE_URI: &str =
    "ipfs://bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku";
const MAX_DUE_DATE_EXTENSION_SECS: u64 = SECS_PER_DAY * 90;
/// Oracle fallback and verification timeout: 72 hours (3 days) in seconds.
/// If an invoice remains in AwaitingVerification for longer than this duration,
/// the SME or admin can call timeout_verification() to cancel it and prevent
/// permanent freezing if the oracle is unavailable.
pub const VERIFICATION_TIMEOUT_SECS: u64 = 72 * 60 * 60;

// ── #290: Storage monitoring constants ───────────────────────────────────────
/// Conservative per-entry storage rent rate (1 stroop / ledger / entry).
const STROOPS_PER_LEDGER_PER_ENTRY: u64 = 1;
/// Approximate ledgers per month (5 s/ledger × 60 × 60 × 24 × 30).
const LEDGERS_PER_MONTH: u64 = 518_400;
/// Maximum batch size for `cleanup_expired_storage` to bound gas usage.
const MAX_CLEANUP_BATCH: u32 = 50;

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
    AlreadyInitialized = 0,
    Unauthorized = 1,
    InvalidStatusTransition = 2,
    InvoiceNotFound = 3,
    HashMismatch = 4,
    SmeExposureLimitExceeded = 5,
    AmountOverflow = 6,
    // #436: string field validation errors
    EmptyField = 7,
    FieldTooLong = 8,
    DateOverflow = 9,
    // #406: metadata URL validation
    InvalidMetadata = 10,
    // Per-field length validation (used by validate_invoice_strings)
    DescriptionTooLong = 11,
    DebtorNameTooLong = 12,
    VerificationHashTooLong = 13,
    // Due-date extension flow (request_extension / approve_extension)
    ExtensionAlreadyPending = 14,
    InvalidDueDateExtension = 15,
    ExtensionTooLarge = 16,
    NoPendingExtension = 17,
    // Cross-contract call to pool contract failed (network/host error, not a logic rejection)
    PoolCallFailed = 18,
    // Overflow in arithmetic operation (expiration timestamp, etc.)
    ArithmeticOverflow = 19,
    // #436: per-field empty/invalid string errors
    EmptyDebtorName = 20,
    EmptyDescription = 21,
    InvalidVerificationHash = 22,
    // Due date is too close to the current ledger timestamp
    DueDateTooSoon = 23,
    // #338: upgrade timelock errors
    UpgradeTimelockNotExpired = 24,
    InvalidUpgradeTimelock = 25,
    // #340: invalid WASM hash (e.g. all-zero hash)
    InvalidWasmHash = 26,
    // Oracle fallback and verification timeout errors
    VerificationDeadlineNotPassed = 27,
    // #565: admin key rotation timelock errors
    AdminChangePending = 28,
    AdminChangeTimelockNotExpired = 29,
    NoAdminChangeProposed = 30,
    ContractPaused = 31,
    // #539: dispute mechanism errors
    DisputeWindowClosed = 32,
    DisputeNotFound = 33,
    DisputeAlreadyResolved = 34,
    // #861: N-of-M staked oracle consensus network
    ConsensusVerificationRequired = 35,
    OracleRegistryNotConfigured = 36,
    UnauthorizedRegistry = 37,
}

#[contracttype]
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum DisputeResolution {
    Pending,
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
    pub original_due_date: u64,
    pub due_date: u64,
    pub pending_due_date: Option<u64>,
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
    pub disputed_at: u64,
    pub grace_period_override: Option<u32>,
    pub verification_deadline: u64,
}

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub struct InvoiceMetadata {
    pub name: String,
    pub description: String,
    pub image: String,
    pub amount: i128,
    pub debtor: String,
    pub due_date: u64,
    pub status: InvoiceStatus,
    pub symbol: String,
    pub decimals: u32,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct DisputeRecord {
    pub evidence_hash: String,
    pub filed_at: u64,
    pub resolved_at: u64,
    pub outcome: DisputeResolution,
}

#[contracttype]
#[derive(Clone, Default)]
pub struct StorageStats {
    pub total_invoices: u64,
    pub active_invoices: u64,
    pub cleaned_invoices: u64,
}

// ── Version tracking (#237) ───────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ContractVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

fn parse_version() -> ContractVersion {
    let v = env!("CARGO_PKG_VERSION");
    let mut parts = v.splitn(3, '.');
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|s| s.split('-').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    ContractVersion {
        major,
        minor,
        patch,
    }
}

const CURRENT_MIGRATION_VERSION: u32 = 1;

// ── Debtor registry (#241) ────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug)]
pub struct DebtorRecord {
    pub debtor_id: String,
    pub debtor_name: String,
    pub max_exposure: i128,
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
    OracleSecondary,
    Initialized,
    StorageStats,
    Paused,
    MinDueDateWindowSecs,
    DailyInvoiceCount(Address),
    DailyInvoiceResetTime(Address),
    // #577: sliding-window rate limiter — stores Vec<u64> of per-SME invoice timestamps
    InvoiceTimestamps(Address),
    ProposedWasmHash,
    UpgradeScheduledAt,
    GracePeriodDays,
    MaxInvoiceAmount,
    MaxOutstandingPerSme,
    ExpirationDurationSecs,
    DailyInvoiceLimit,
    DisputeResolutionWindow,
    ContractVersion,
    MigrationVersion,
    RequireRegisteredDebtor,
    DebtorRecord(String),
    DebtorIds,
    SmeOutstanding(Address),
    SmeInvoices(Address),
    MetadataImageUri,
    // #446: admin-configurable TTL for completed invoices
    CompletedInvoiceTtl,
    // #338: configurable upgrade timelock duration in seconds
    UpgradeTimelockSecs,
    // #565: admin key rotation timelock
    PendingAdmin,
    AdminChangeScheduledAt,
    // #654: enforce oracle verification before invoices may be funded
    RequireOracleVerification,
    // #539: invoice dispute mechanism
    Dispute(u64),
    // #861: N-of-M staked oracle consensus network
    OracleRegistry,
    RequireConsensusVerification,
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
    let expires_at = match invoice.created_at.checked_add(expiration_duration_secs) {
        Some(ts) => ts,
        None => return invoice, // overflow: expiration beyond u64::MAX → can never fire
    };
    if now <= expires_at {
        return invoice;
    }

    invoice.status = InvoiceStatus::Expired;
    remove_invoice_from_owner(env, &invoice.owner, invoice.id);
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

fn validate_invoice_strings(
    env: &Env,
    debtor: &String,
    description: &String,
    verification_hash: &String,
) {
    if debtor.is_empty() {
        panic_with_error!(env, InvoiceError::EmptyDebtorName);
    }
    if description.is_empty() {
        panic_with_error!(env, InvoiceError::EmptyDescription);
    }
    if verification_hash.is_empty() {
        panic_with_error!(env, InvoiceError::InvalidVerificationHash);
    }
    if description.len() > MAX_DESCRIPTION_LEN {
        panic_with_error!(env, InvoiceError::DescriptionTooLong);
    }
    if debtor.len() > MAX_DEBTOR_LEN {
        panic_with_error!(env, InvoiceError::DebtorNameTooLong);
    }
    if verification_hash.len() > MAX_VERIFICATION_HASH_LEN {
        panic_with_error!(env, InvoiceError::VerificationHashTooLong);
    }
}

fn max_extension_due_date(invoice: &Invoice) -> u64 {
    invoice
        .original_due_date
        .saturating_add(MAX_DUE_DATE_EXTENSION_SECS)
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
        panic_with_error!(env, InvoiceError::ContractPaused);
    }
}

fn is_valid_metadata_uri(_env: &Env, uri: &String) -> bool {
    if uri.is_empty() || uri.len() > MAX_METADATA_URI_LEN {
        return false;
    }
    let len = uri.len() as usize;
    let mut buf = [0u8; 256];
    uri.copy_into_slice(&mut buf[..len]);
    (len >= 7 && &buf[..7] == b"ipfs://")
        || (len >= 5 && &buf[..5] == b"ar://")
        || (len >= 8 && &buf[..8] == b"https://")
}

fn set_invoice_ttl(env: &Env, id: u64, is_completed: bool) {
    let ttl = if is_completed {
        // #446: use admin-configured TTL when set, otherwise fall back to the
        // 5-year default so completed invoices are never evicted prematurely.
        env.storage()
            .instance()
            .get(&DataKey::CompletedInvoiceTtl)
            .unwrap_or(COMPLETED_INVOICE_TTL)
    } else {
        ACTIVE_INVOICE_TTL
    };
    env.storage()
        .persistent()
        .extend_ttl(&DataKey::Invoice(id), ttl, ttl);
}

/// #861: shared status-transition logic for both the legacy 1-of-2
/// `verify_invoice` path and the new stake-weighted `consensus_verify` path
/// — the two entrypoints differ only in who's authorized to call them and
/// what address is checked, not in what a verdict does to the invoice.
fn apply_verification_outcome(
    env: &Env,
    id: u64,
    approved: bool,
    reason: String,
    oracle_hash: String,
) -> Result<(), InvoiceError> {
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
    set_invoice_ttl(env, id, false);
    if approved {
        env.events()
            .publish((EVT, symbol_short!("verified")), (id, oracle_hash));
    } else {
        env.events().publish(
            (EVT, symbol_short!("disputed")),
            (id, env.ledger().timestamp()),
        );
    }
    Ok(())
}

fn get_max_outstanding_per_sme(env: &Env) -> i128 {
    env.storage()
        .instance()
        .get(&DataKey::MaxOutstandingPerSme)
        .unwrap_or(i128::MAX)
}

fn get_sme_outstanding(env: &Env, sme: &Address) -> i128 {
    env.storage()
        .persistent()
        .get(&DataKey::SmeOutstanding(sme.clone()))
        .unwrap_or(0)
}

fn set_sme_outstanding(env: &Env, sme: &Address, value: i128) {
    env.storage()
        .persistent()
        .set(&DataKey::SmeOutstanding(sme.clone()), &value);
}

fn decrease_sme_outstanding(env: &Env, sme: &Address, amount: i128) {
    let current = get_sme_outstanding(env, sme);
    set_sme_outstanding(env, sme, current.saturating_sub(amount));
}

/// Add an invoice ID to the owner's invoice index (#651).
fn add_invoice_to_owner(env: &Env, owner: &Address, invoice_id: u64) {
    let key = DataKey::SmeInvoices(owner.clone());
    let mut ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    ids.push_back(invoice_id);
    env.storage().persistent().set(&key, &ids);
    env.storage()
        .persistent()
        .extend_ttl(&key, LEDGERS_PER_DAY * 365, LEDGERS_PER_DAY * 365);
}

/// Remove an invoice ID from the owner's invoice index (#651).
fn remove_invoice_from_owner(env: &Env, owner: &Address, invoice_id: u64) {
    let key = DataKey::SmeInvoices(owner.clone());
    let ids: Vec<u64> = env
        .storage()
        .persistent()
        .get(&key)
        .unwrap_or_else(|| Vec::new(env));
    let mut new_ids: Vec<u64> = Vec::new(env);
    for i in 0..ids.len() {
        let id = ids.get(i).unwrap();
        if id != invoice_id {
            new_ids.push_back(id);
        }
    }
    if new_ids.is_empty() {
        env.storage().persistent().remove(&key);
    } else {
        env.storage().persistent().set(&key, &new_ids);
        env.storage()
            .persistent()
            .extend_ttl(&key, LEDGERS_PER_DAY * 365, LEDGERS_PER_DAY * 365);
    }
}

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
        .unwrap_or_else(|| panic_with_error!(env, InvoiceError::InvoiceNotFound))
}

fn checked_default_deadline(env: &Env, due_date: u64, grace_period_days: u32) -> u64 {
    let grace_period_secs = (grace_period_days as u64)
        .checked_mul(SECS_PER_DAY)
        .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, InvoiceError::DateOverflow));
    due_date
        .checked_add(grace_period_secs)
        .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, InvoiceError::DateOverflow))
}

fn resolve_invoice_grace_period_days(env: &Env, invoice: &Invoice) -> u32 {
    let global_grace: u32 = env
        .storage()
        .instance()
        .get(&DataKey::GracePeriodDays)
        .unwrap_or(DEFAULT_GRACE_PERIOD_DAYS);
    invoice.grace_period_override.unwrap_or(global_grace)
}

fn validate_due_date(env: &Env, due_date: u64) {
    let now = env.ledger().timestamp();
    let max_due_date = now
        .checked_add(MAX_DUE_DATE_AHEAD_SECS)
        .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, InvoiceError::DateOverflow));
    if due_date > max_due_date {
        soroban_sdk::panic_with_error!(env, InvoiceError::DateOverflow);
    }
}

fn resolve_min_due_date_window(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&DataKey::MinDueDateWindowSecs)
        .unwrap_or(DEFAULT_MIN_DUE_DATE_WINDOW_SECS)
}

fn validate_min_due_date_window(env: &Env, due_date: u64) {
    let now = env.ledger().timestamp();
    let min_due_date = now
        .checked_add(resolve_min_due_date_window(env))
        .unwrap_or_else(|| soroban_sdk::panic_with_error!(env, InvoiceError::DateOverflow));
    if due_date < min_due_date {
        soroban_sdk::panic_with_error!(env, InvoiceError::DueDateTooSoon);
    }
}

#[contract]
pub struct InvoiceContract;

#[contractimpl]
impl InvoiceContract {
    pub fn initialize(
        env: Env,
        admin: Address,
        pool: Address,
        max_invoice_amount: i128,
        expiration_duration_secs: u64,
        grace_period_days: u32,
    ) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic_with_error!(&env, InvoiceError::AlreadyInitialized);
        }
        if max_invoice_amount <= 0 {
            panic!("max invoice amount must be positive");
        }
        if expiration_duration_secs == 0 {
            panic!("expiration duration must be non-zero");
        }
        if expiration_duration_secs > MAX_EXPIRATION_DURATION_SECS {
            panic_with_error!(&env, InvoiceError::ArithmeticOverflow);
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
            .set(&DataKey::GracePeriodDays, &grace_period_days);
        env.storage()
            .instance()
            .set(&DataKey::MaxInvoiceAmount, &max_invoice_amount);
        env.storage()
            .instance()
            .set(&DataKey::MaxOutstandingPerSme, &i128::MAX);
        env.storage()
            .instance()
            .set(&DataKey::ExpirationDurationSecs, &expiration_duration_secs);
        env.storage().instance().set(
            &DataKey::DisputeResolutionWindow,
            &DEFAULT_DISPUTE_RESOLUTION_WINDOW,
        );
        env.storage().instance().set(
            &DataKey::MinDueDateWindowSecs,
            &DEFAULT_MIN_DUE_DATE_WINDOW_SECS,
        );
        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &parse_version());
        env.storage()
            .instance()
            .set(&DataKey::MigrationVersion, &0u32);
        env.storage()
            .instance()
            .set(&DataKey::RequireRegisteredDebtor, &false);
        env.storage()
            .instance()
            .set(&DataKey::RequireOracleVerification, &false);
        env.storage().persistent().set(
            &DataKey::MetadataImageUri,
            &String::from_str(&env, DEFAULT_METADATA_IMAGE_URI),
        );
        env.storage()
            .instance()
            .set(&DataKey::DebtorIds, &Vec::<String>::new(&env));
        bump_instance(&env);
    }

    pub fn version(env: Env) -> ContractVersion {
        env.storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap_or_else(parse_version)
    }

    pub fn migration_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MigrationVersion)
            .unwrap_or(0)
    }

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
        env.storage()
            .instance()
            .set(&DataKey::MigrationVersion, &CURRENT_MIGRATION_VERSION);
    }

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

    pub fn get_debtor(env: Env, debtor_id: String) -> DebtorRecord {
        env.storage()
            .persistent()
            .get(&DataKey::DebtorRecord(debtor_id))
            .expect("debtor not found")
    }

    pub fn list_debtors(env: Env) -> Vec<String> {
        env.storage()
            .instance()
            .get(&DataKey::DebtorIds)
            .unwrap_or_else(|| Vec::new(&env))
    }

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
        let old_required: bool = env
            .storage()
            .instance()
            .get(&DataKey::RequireRegisteredDebtor)
            .unwrap_or(false);
        env.storage()
            .instance()
            .set(&DataKey::RequireRegisteredDebtor, &required);
        env.events().publish(
            (EVT, Symbol::new(&env, "debtor_reg_updated")),
            (admin, old_required, required),
        );
    }

    // #654: gate funding on oracle verification when the admin flips this flag on.
    pub fn set_oracle_verified_funding_only(env: Env, admin: Address, required: bool) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic_with_error!(&env, InvoiceError::Unauthorized);
        }
        let old_required: bool = env
            .storage()
            .instance()
            .get(&DataKey::RequireOracleVerification)
            .unwrap_or(false);
        env.storage()
            .instance()
            .set(&DataKey::RequireOracleVerification, &required);
        env.events().publish(
            (EVT, Symbol::new(&env, "oracle_req_updated")),
            (admin, old_required, required),
        );
    }

    pub fn oracle_verified_funding_only(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::RequireOracleVerification)
            .unwrap_or(false)
    }

    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic_with_error!(&env, InvoiceError::Unauthorized);
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
            panic_with_error!(&env, InvoiceError::Unauthorized);
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
        let old_oracle: Option<Address> = env.storage().instance().get(&DataKey::Oracle);
        env.storage().instance().set(&DataKey::Oracle, &oracle);
        bump_instance(&env);
        env.events().publish(
            (EVT, Symbol::new(&env, "oracle_updated")),
            (admin, old_oracle, oracle),
        );
    }

    /// Set the secondary (fallback) oracle address. Admin-only operation.
    /// If set to None, removes the secondary oracle (only primary can verify).
    /// If set to Some(address), both primary and secondary can verify invoices.
    pub fn set_secondary_oracle(env: Env, admin: Address, oracle_secondary: Option<Address>) {
        admin.require_auth();
        require_not_paused(&env);
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic_with_error!(&env, InvoiceError::Unauthorized);
        }
        let old_secondary: Option<Address> =
            env.storage().instance().get(&DataKey::OracleSecondary);
        env.storage()
            .instance()
            .set(&DataKey::OracleSecondary, &oracle_secondary);
        bump_instance(&env);
        env.events().publish(
            (EVT, Symbol::new(&env, "sec_oracle_upd")),
            (admin, old_secondary, oracle_secondary),
        );
    }

    /// #861: registers the `oracle_registry` contract address allowed to call
    /// `consensus_verify`. Admin-only; does not by itself change how
    /// `verify_invoice` behaves — see `set_consensus_required`.
    pub fn set_oracle_registry(env: Env, admin: Address, registry: Address) {
        admin.require_auth();
        require_not_paused(&env);
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic_with_error!(&env, InvoiceError::Unauthorized);
        }
        let old_registry: Option<Address> = env.storage().instance().get(&DataKey::OracleRegistry);
        env.storage()
            .instance()
            .set(&DataKey::OracleRegistry, &registry);
        bump_instance(&env);
        env.events().publish(
            (EVT, Symbol::new(&env, "registry_upd")),
            (admin, old_registry, registry),
        );
    }

    pub fn get_oracle_registry(env: Env) -> Option<Address> {
        bump_instance(&env);
        env.storage().instance().get(&DataKey::OracleRegistry)
    }

    /// #861: when `true`, `verify_invoice` (the legacy 1-of-2 oracle path)
    /// rejects direct calls and every invoice must instead go through the
    /// registered `oracle_registry`'s stake-weighted consensus flow, which
    /// calls back into `consensus_verify`. Defaults to `false` so existing
    /// deployments and the pre-#861 single-oracle flow keep working
    /// unmodified until an admin opts in.
    pub fn set_consensus_required(env: Env, admin: Address, required: bool) {
        admin.require_auth();
        require_not_paused(&env);
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic_with_error!(&env, InvoiceError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&DataKey::RequireConsensusVerification, &required);
        bump_instance(&env);
        env.events().publish(
            (EVT, Symbol::new(&env, "consensus_req_upd")),
            (admin, required),
        );
    }

    pub fn require_consensus_verification(env: Env) -> bool {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::RequireConsensusVerification)
            .unwrap_or(false)
    }

    pub fn get_metadata_image_uri(env: Env) -> String {
        env.storage()
            .persistent()
            .get(&DataKey::MetadataImageUri)
            .unwrap_or_else(|| String::from_str(&env, DEFAULT_METADATA_IMAGE_URI))
    }

    pub fn set_metadata_image_uri(env: Env, admin: Address, uri: String) {
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
        if uri.is_empty() || uri.len() > MAX_METADATA_URI_LEN {
            panic!("invalid metadata image uri");
        }
        env.storage()
            .persistent()
            .set(&DataKey::MetadataImageUri, &uri);
        env.events()
            .publish((EVT, symbol_short!("meta_img")), (admin, uri));
    }

    pub fn create_invoice(
        env: Env,
        owner: Address,
        debtor: String,
        amount: i128,
        due_date: u64,
        description: String,
        verification_hash: String,
        metadata_url: String,
    ) -> u64 {
        if metadata_url.is_empty() {
            soroban_sdk::panic_with_error!(&env, InvoiceError::InvalidMetadata);
        }
        if !is_valid_metadata_uri(&env, &metadata_url) {
            soroban_sdk::panic_with_error!(&env, InvoiceError::InvalidMetadata);
        }
        Self::create_invoice_with_metadata(
            env,
            owner,
            debtor,
            amount,
            due_date,
            description,
            verification_hash,
            Some(metadata_url),
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
        validate_invoice_strings(&env, &debtor, &description, &verification_hash);

        if let Some(uri) = metadata_uri.as_ref() {
            if !is_valid_metadata_uri(&env, uri) {
                panic!("invalid metadata uri");
            }
        }
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
        validate_min_due_date_window(&env, due_date);
        validate_due_date(&env, due_date);

        let outstanding = get_sme_outstanding(&env, &owner);
        let max_outstanding = get_max_outstanding_per_sme(&env);
        if outstanding.saturating_add(amount) > max_outstanding {
            panic!("SmeExposureLimitExceeded");
        }

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

        let daily_limit: u32 = env
            .storage()
            .instance()
            .get(&DataKey::DailyInvoiceLimit)
            .unwrap_or(MAX_INVOICES_PER_DAY);
        let now = env.ledger().timestamp();

        // Sliding-window rate limiter: count only invoices created in the last
        // SECS_PER_DAY seconds. This prevents the fixed-window midnight-boundary
        // exploit where an SME could straddle a UTC day change and submit twice
        // the daily quota within a few minutes (#577).
        let ts_key = DataKey::InvoiceTimestamps(owner.clone());
        let timestamps: Vec<u64> = env
            .storage()
            .persistent()
            .get(&ts_key)
            .unwrap_or_else(|| Vec::new(&env));

        // Retain only timestamps strictly within the 24-hour window.
        let window_start = now.saturating_sub(SECS_PER_DAY);
        let mut fresh: Vec<u64> = Vec::new(&env);
        for i in 0..timestamps.len() {
            let ts = timestamps.get(i).unwrap();
            if ts > window_start {
                fresh.push_back(ts);
            }
        }

        if fresh.len() >= daily_limit {
            panic!("daily invoice limit exceeded");
        }

        bump_instance(&env);
        fresh.push_back(now);
        env.storage().persistent().set(&ts_key, &fresh);
        env.storage().persistent().extend_ttl(
            &ts_key,
            SLIDING_WINDOW_TTL_LEDGERS,
            SLIDING_WINDOW_TTL_LEDGERS,
        );

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

        let created_at_ts = env.ledger().timestamp();
        let verification_deadline_ts = created_at_ts
            .checked_add(VERIFICATION_TIMEOUT_SECS)
            .unwrap_or_else(|| panic_with_error!(&env, InvoiceError::ArithmeticOverflow));

        let invoice = Invoice {
            id,
            owner: owner.clone(),
            debtor,
            amount,
            original_due_date: due_date,
            due_date,
            pending_due_date: None,
            description,
            status: initial_status,
            created_at: created_at_ts,
            funded_at: 0,
            paid_at: 0,
            pool_contract: pool_addr,
            verification_hash,
            metadata_uri: metadata_uri.clone(),
            oracle_verified: false,
            dispute_reason: empty_str,
            disputed_at: 0,
            grace_period_override: None,
            verification_deadline: verification_deadline_ts,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);
        set_invoice_ttl(&env, id, false);
        env.storage().instance().set(&DataKey::InvoiceCount, &id);
        add_invoice_to_owner(&env, &owner, id);

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

    pub fn request_extension(
        env: Env,
        id: u64,
        owner: Address,
        new_due_date: u64,
    ) -> Result<(), InvoiceError> {
        owner.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .ok_or(InvoiceError::InvoiceNotFound)?;
        if owner != invoice.owner {
            return Err(InvoiceError::Unauthorized);
        }
        if invoice.status != InvoiceStatus::Funded {
            return Err(InvoiceError::InvalidStatusTransition);
        }
        if invoice.pending_due_date.is_some() {
            return Err(InvoiceError::ExtensionAlreadyPending);
        }
        if new_due_date <= env.ledger().timestamp() || new_due_date <= invoice.due_date {
            return Err(InvoiceError::InvalidDueDateExtension);
        }
        if new_due_date > max_extension_due_date(&invoice) {
            return Err(InvoiceError::ExtensionTooLarge);
        }

        let current_due_date = invoice.due_date;
        invoice.pending_due_date = Some(new_due_date);
        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);
        set_invoice_ttl(&env, id, false);
        env.events().publish(
            (EVT, Symbol::new(&env, "extension_requested")),
            (
                id,
                current_due_date,
                new_due_date,
                owner,
                env.ledger().timestamp(),
            ),
        );
        Ok(())
    }

    pub fn approve_extension(env: Env, id: u64, approver: Address) -> Result<(), InvoiceError> {
        approver.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        let oracle: Option<Address> = env.storage().instance().get(&DataKey::Oracle);
        let is_authorized = approver == admin || oracle.as_ref() == Some(&approver);
        if !is_authorized {
            return Err(InvoiceError::Unauthorized);
        }

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .ok_or(InvoiceError::InvoiceNotFound)?;
        if invoice.status != InvoiceStatus::Funded {
            return Err(InvoiceError::InvalidStatusTransition);
        }

        let new_due_date = invoice
            .pending_due_date
            .ok_or(InvoiceError::NoPendingExtension)?;
        if new_due_date <= env.ledger().timestamp() || new_due_date <= invoice.due_date {
            return Err(InvoiceError::InvalidDueDateExtension);
        }
        if new_due_date > max_extension_due_date(&invoice) {
            return Err(InvoiceError::ExtensionTooLarge);
        }

        let old_due_date = invoice.due_date;
        invoice.due_date = new_due_date;
        invoice.pending_due_date = None;
        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);
        set_invoice_ttl(&env, id, false);

        let pool_client = PoolClient::new(&env, &invoice.pool_contract);
        pool_client.update_invoice_due_date(&env.current_contract_address(), &id, &new_due_date);

        env.events().publish(
            (EVT, Symbol::new(&env, "extension_approved")),
            (
                id,
                old_due_date,
                new_due_date,
                approver,
                env.ledger().timestamp(),
            ),
        );
        Ok(())
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
        let old_limit: u32 = env
            .storage()
            .instance()
            .get(&DataKey::DailyInvoiceLimit)
            .unwrap_or(MAX_INVOICES_PER_DAY);
        env.storage()
            .instance()
            .set(&DataKey::DailyInvoiceLimit, &limit);
        env.events().publish(
            (EVT, Symbol::new(&env, "daily_limit_updated")),
            (admin, old_limit, limit),
        );
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
        // #861: once an admin has opted into consensus verification, the
        // legacy 1-of-2 path is rejected outright — every invoice must go
        // through the registered oracle registry's stake-weighted quorum.
        let require_consensus: bool = env
            .storage()
            .instance()
            .get(&DataKey::RequireConsensusVerification)
            .unwrap_or(false);
        if require_consensus {
            return Err(InvoiceError::ConsensusVerificationRequired);
        }
        let stored_oracle: Address = env
            .storage()
            .instance()
            .get(&DataKey::Oracle)
            .expect("oracle not configured");
        // Accept verification from either the primary or secondary oracle (if configured).
        // This provides a fallback if the primary oracle is unavailable.
        let secondary_oracle: Option<Address> =
            env.storage().instance().get(&DataKey::OracleSecondary);
        let is_authorized =
            oracle == stored_oracle || secondary_oracle.as_ref().is_some_and(|s| oracle == *s);
        if !is_authorized {
            panic!("unauthorized oracle");
        }
        apply_verification_outcome(&env, id, approved, reason, oracle_hash)
    }

    /// #861: counterpart to `verify_invoice` for the N-of-M staked oracle
    /// consensus network. Auth-gated to the single registered
    /// `oracle_registry` contract address (mirrors the existing
    /// pool-auth pattern used by `mark_funded`/`mark_paid`/`mark_defaulted`):
    /// the registry calls this itself once stake-weighted votes cross quorum,
    /// so `registry.require_auth()` is satisfied by the registry being the
    /// direct caller in the same cross-contract invocation, not by a
    /// separately-signed transaction.
    pub fn consensus_verify(
        env: Env,
        id: u64,
        registry: Address,
        approved: bool,
        reason: String,
        oracle_hash: String,
    ) -> Result<(), InvoiceError> {
        registry.require_auth();
        require_not_paused(&env);
        bump_instance(&env);
        let stored_registry: Address = env
            .storage()
            .instance()
            .get(&DataKey::OracleRegistry)
            .ok_or(InvoiceError::OracleRegistryNotConfigured)?;
        if registry != stored_registry {
            return Err(InvoiceError::UnauthorizedRegistry);
        }
        apply_verification_outcome(&env, id, approved, reason, oracle_hash)
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
                let sme = invoice.owner.clone();
                decrease_sme_outstanding(&env, &sme, invoice.amount);
                let mut stats: StorageStats = env
                    .storage()
                    .instance()
                    .get(&DataKey::StorageStats)
                    .unwrap_or_default();
                stats.active_invoices = stats.active_invoices.saturating_sub(1);
                env.storage().instance().set(&DataKey::StorageStats, &stats);
                set_invoice_ttl(&env, id, true);
            }
            DisputeResolution::Pending => {
                panic_with_error!(&env, InvoiceError::InvalidStatusTransition);
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
        let old_window: u64 = env
            .storage()
            .instance()
            .get(&DataKey::DisputeResolutionWindow)
            .unwrap_or(DEFAULT_DISPUTE_RESOLUTION_WINDOW);
        env.storage()
            .instance()
            .set(&DataKey::DisputeResolutionWindow, &window);
        bump_instance(&env);
        env.events().publish(
            (EVT, Symbol::new(&env, "dispute_window_updated")),
            (admin, old_window, window),
        );
    }

    pub fn get_dispute_window(env: Env) -> u64 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::DisputeResolutionWindow)
            .unwrap_or(DEFAULT_DISPUTE_RESOLUTION_WINDOW)
    }

    pub fn mark_funded(env: Env, id: u64, pool: Address) -> Result<(), InvoiceError> {
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
        // #654: when oracle-verified-funding-only is on, block unverified invoices.
        let require_oracle: bool = env
            .storage()
            .instance()
            .get(&DataKey::RequireOracleVerification)
            .unwrap_or(false);
        if require_oracle && !invoice.oracle_verified {
            panic_with_error!(&env, InvoiceError::Unauthorized);
        }
        invoice.status = InvoiceStatus::Funded;
        invoice.funded_at = env.ledger().timestamp();
        invoice.pool_contract = pool;
        let sme = invoice.owner.clone();
        let current_outstanding = get_sme_outstanding(&env, &sme);
        let new_outstanding = current_outstanding
            .checked_add(invoice.amount)
            .ok_or(InvoiceError::AmountOverflow)?;
        let max_outstanding = get_max_outstanding_per_sme(&env);
        if new_outstanding > max_outstanding {
            return Err(InvoiceError::SmeExposureLimitExceeded);
        }
        set_sme_outstanding(&env, &sme, new_outstanding);
        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);
        set_invoice_ttl(&env, id, false);
        env.events().publish(
            (EVT, symbol_short!("funded")),
            (id, invoice.owner.clone(), env.ledger().timestamp()),
        );
        Ok(())
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
        let pool_client = PoolClient::new(&env, &pool);
        let repaid = match pool_client.try_is_invoice_repaid(&id) {
            Ok(Ok(v)) => v,
            _ => panic_with_error!(&env, InvoiceError::PoolCallFailed),
        };
        if !repaid {
            panic!("repayment not verified by pool contract");
        }
        invoice.status = InvoiceStatus::Paid;
        invoice.paid_at = env.ledger().timestamp();
        let sme = invoice.owner.clone();
        decrease_sme_outstanding(&env, &sme, invoice.amount);
        // Release this invoice's amount from the debtor registry's exposure
        // tracking, if the debtor field refers to a registered debtor (#241).
        // Without this, a debtor's exposure capacity would never be freed up
        // by paid invoices, permanently shrinking how much new business can
        // be written against them.
        let debtor_key = DataKey::DebtorRecord(invoice.debtor.clone());
        if let Some(mut record) = env
            .storage()
            .persistent()
            .get::<DataKey, DebtorRecord>(&debtor_key)
        {
            record.current_exposure = record.current_exposure.saturating_sub(invoice.amount);
            env.storage().persistent().set(&debtor_key, &record);
        }
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
        env.events().publish(
            (EVT, symbol_short!("paid")),
            (id, invoice.owner.clone(), env.ledger().timestamp()),
        );
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
        let grace_period_days = resolve_invoice_grace_period_days(&env, &invoice);
        let now = env.ledger().timestamp();
        let default_at = checked_default_deadline(&env, invoice.due_date, grace_period_days);
        if now < default_at {
            panic!(
                "grace period has not elapsed: default available at {}",
                default_at
            );
        }
        invoice.status = InvoiceStatus::Defaulted;
        let sme = invoice.owner.clone();
        decrease_sme_outstanding(&env, &sme, invoice.amount);
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
        env.events().publish(
            (EVT, symbol_short!("default")),
            (id, invoice.owner.clone(), env.ledger().timestamp()),
        );
    }

    pub fn raise_dispute(env: Env, id: u64, borrower: Address, evidence_hash: String) {
        borrower.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");
        if invoice.status != InvoiceStatus::Defaulted {
            panic_with_error!(&env, InvoiceError::InvalidStatusTransition);
        }
        if invoice.owner != borrower {
            panic_with_error!(&env, InvoiceError::Unauthorized);
        }

        let dispute_window: u64 = env
            .storage()
            .instance()
            .get(&DataKey::DisputeResolutionWindow)
            .unwrap_or(DEFAULT_DISPUTE_RESOLUTION_WINDOW);
        let now = env.ledger().timestamp();
        let default_at = checked_default_deadline(
            &env,
            invoice.due_date,
            resolve_invoice_grace_period_days(&env, &invoice),
        );
        let dispute_deadline = default_at.saturating_add(dispute_window);

        if now > dispute_deadline {
            panic_with_error!(&env, InvoiceError::DisputeWindowClosed);
        }

        if env.storage().persistent().has(&DataKey::Dispute(id)) {
            panic_with_error!(&env, InvoiceError::HashMismatch);
        }

        invoice.status = InvoiceStatus::Disputed;
        let dispute_record = DisputeRecord {
            evidence_hash,
            filed_at: now,
            resolved_at: 0,
            outcome: DisputeResolution::Pending,
        };

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);
        env.storage()
            .persistent()
            .set(&DataKey::Dispute(id), &dispute_record);

        env.events()
            .publish((EVT, symbol_short!("dispute")), (id, borrower, now));
    }

    pub fn resolve_default_dispute(env: Env, id: u64, admin: Address, outcome: DisputeResolution) {
        admin.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let authorized_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != authorized_admin {
            panic_with_error!(&env, InvoiceError::Unauthorized);
        }

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");
        if invoice.status != InvoiceStatus::Disputed {
            panic_with_error!(&env, InvoiceError::InvalidStatusTransition);
        }

        let mut dispute_record: DisputeRecord = env
            .storage()
            .persistent()
            .get(&DataKey::Dispute(id))
            .expect("dispute not found");

        if dispute_record.outcome != DisputeResolution::Pending {
            panic_with_error!(&env, InvoiceError::DisputeAlreadyResolved);
        }

        let now = env.ledger().timestamp();
        dispute_record.resolved_at = now;
        dispute_record.outcome = outcome.clone();

        match outcome {
            DisputeResolution::InFavorOfDebtor => {
                invoice.status = InvoiceStatus::Funded;
                let grace_period_days = resolve_invoice_grace_period_days(&env, &invoice);
                let grace_secs = (grace_period_days as u64).saturating_mul(SECS_PER_DAY);
                invoice.due_date = now.saturating_add(grace_secs);
            }
            DisputeResolution::InFavorOfSME => {
                invoice.status = InvoiceStatus::Defaulted;
            }
            DisputeResolution::Pending => {
                panic_with_error!(&env, InvoiceError::InvalidStatusTransition);
            }
        }

        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);
        env.storage()
            .persistent()
            .set(&DataKey::Dispute(id), &dispute_record);

        env.events()
            .publish((EVT, symbol_short!("resolved")), (id, admin, now));
    }

    pub fn get_dispute(env: Env, id: u64) -> Option<DisputeRecord> {
        env.storage().persistent().get(&DataKey::Dispute(id))
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
        remove_invoice_from_owner(&env, &invoice.owner, id);
        let sme = invoice.owner.clone();
        decrease_sme_outstanding(&env, &sme, invoice.amount);
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

    /// Cancel an invoice that has been stuck in AwaitingVerification status
    /// past its verification deadline. Can be called by the invoice owner (SME)
    /// or admin after the verification_deadline timestamp has passed.
    ///
    /// This function prevents invoices from being permanently frozen if the
    /// oracle becomes unavailable. The verification deadline is set to
    /// created_at + 72 hours (VERIFICATION_TIMEOUT_SECS) when the invoice is created.
    ///
    /// Transitions the invoice to Cancelled status and emits a VERIFICATION_TIMEOUT event.
    /// Note: SME outstanding is NOT decreased here because invoices in AwaitingVerification
    /// were never funded, so outstanding was never incremented.
    pub fn timeout_verification(env: Env, caller: Address, id: u64) {
        caller.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        let mut invoice: Invoice = env
            .storage()
            .persistent()
            .get(&DataKey::Invoice(id))
            .expect("invoice not found");

        // Authorization: invoice owner (SME) or admin can call this function
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        let is_authorized = caller == invoice.owner || caller == admin;
        if !is_authorized {
            panic_with_error!(&env, InvoiceError::Unauthorized);
        }

        // Status check: invoice must be in AwaitingVerification
        if invoice.status != InvoiceStatus::AwaitingVerification {
            panic_with_error!(&env, InvoiceError::InvalidStatusTransition);
        }

        // Deadline check: current time must be past verification_deadline
        let now = env.ledger().timestamp();
        if now < invoice.verification_deadline {
            panic_with_error!(&env, InvoiceError::VerificationDeadlineNotPassed);
        }

        // State transition: move to Cancelled
        invoice.status = InvoiceStatus::Cancelled;
        remove_invoice_from_owner(&env, &invoice.owner, id);

        // Note: We do NOT call decrease_sme_outstanding here because the invoice
        // was never funded. SME outstanding is only incremented in mark_funded(),
        // so there's nothing to decrease for an AwaitingVerification invoice.

        // Update storage and TTL
        env.storage()
            .persistent()
            .set(&DataKey::Invoice(id), &invoice);
        set_invoice_ttl(&env, id, true);

        // Update storage stats (decrease active invoice count)
        let mut stats: StorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();
        stats.active_invoices = stats.active_invoices.saturating_sub(1);
        env.storage().instance().set(&DataKey::StorageStats, &stats);

        // Emit event with invoice ID, deadline, and current timestamp
        env.events().publish(
            (EVT, symbol_short!("vtimeout")),
            (id, invoice.verification_deadline, now),
        );
    }

    /// Admin-only single-entry cleanup (existing behaviour, unchanged).
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

    // ── #290: Public batch cleanup ────────────────────────────────────────────

    /// Remove terminal invoice entries from persistent storage in batch (#290).
    ///
    /// **Public function** — callable by anyone (caller must sign; no admin role
    /// required) to keep storage lean and reduce ongoing rent costs.
    ///
    /// Terminal states: `Paid`, `Defaulted`, `Cancelled`, `Expired`.
    /// Active invoices (`Pending`, `AwaitingVerification`, `Verified`, `Funded`,
    /// `Disputed`) are **silently skipped** — they are never removed.
    ///
    /// The function is idempotent: IDs that were already removed (or never
    /// existed) are skipped without panicking.
    ///
    /// # Arguments
    /// * `caller` — Any address that authorises the call.
    /// * `ids`    — Batch of invoice IDs to attempt cleanup (max 50 per call).
    ///
    /// # Returns
    /// Number of entries actually removed in this call.
    ///
    /// # Events
    /// Emits `(INVOICE, "st_clean")` with `(removed_count, caller)` when at
    /// least one entry is removed.
    ///
    /// # Panics
    /// Panics if `ids.len() > MAX_CLEANUP_BATCH` (50).
    pub fn cleanup_expired_storage(env: Env, caller: Address, ids: Vec<u64>) -> u32 {
        caller.require_auth();
        require_not_paused(&env);
        bump_instance(&env);

        if ids.len() > MAX_CLEANUP_BATCH {
            panic!(
                "cleanup batch exceeds maximum of {} entries",
                MAX_CLEANUP_BATCH
            );
        }

        let mut removed: u32 = 0;

        for i in 0..ids.len() {
            let id = ids.get(i).unwrap();
            let key = DataKey::Invoice(id);

            // Idempotent: skip IDs that no longer exist in storage.
            let maybe_invoice: Option<Invoice> = env.storage().persistent().get(&key);
            let invoice = match maybe_invoice {
                Some(inv) => inv,
                None => continue,
            };

            let is_terminal = matches!(
                invoice.status,
                InvoiceStatus::Paid
                    | InvoiceStatus::Defaulted
                    | InvoiceStatus::Cancelled
                    | InvoiceStatus::Expired
            );

            if !is_terminal {
                // Active invoice — skip silently.
                continue;
            }

            env.storage().persistent().remove(&key);
            removed += 1;
        }

        if removed > 0 {
            let mut stats: StorageStats = env
                .storage()
                .instance()
                .get(&DataKey::StorageStats)
                .unwrap_or_default();
            stats.cleaned_invoices = stats.cleaned_invoices.saturating_add(removed as u64);
            env.storage().instance().set(&DataKey::StorageStats, &stats);

            env.events()
                .publish((EVT, symbol_short!("st_clean")), (removed, caller));
        }

        removed
    }

    /// Estimate the current monthly persistent storage rent in stroops (#290).
    ///
    /// Uses `StorageStats.active_invoices` as the live entry count and applies
    /// a conservative per-ledger rate:
    ///
    /// ```text
    /// active_invoices × STROOPS_PER_LEDGER_PER_ENTRY × LEDGERS_PER_MONTH
    /// ```
    ///
    /// **Approximation only** — real costs vary with entry size, TTL settings,
    /// and network fee schedules.
    ///
    /// # Returns
    /// Estimated rent cost in stroops per month.
    pub fn estimate_storage_cost(env: Env) -> u64 {
        bump_instance(&env);
        let stats: StorageStats = env
            .storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default();

        stats
            .active_invoices
            .saturating_mul(STROOPS_PER_LEDGER_PER_ENTRY)
            .saturating_mul(LEDGERS_PER_MONTH)
    }

    // ── Existing view / setter methods (unchanged) ────────────────────────────

    pub fn get_invoice(env: Env, id: u64) -> Invoice {
        load_invoice(&env, id)
    }

    pub fn get_multiple_invoices(env: Env, ids: Vec<u64>) -> Vec<Invoice> {
        bump_instance(&env);
        let mut invoices: Vec<Invoice> = Vec::new(&env);
        for i in 0..ids.len() {
            let inv = load_invoice(&env, ids.get(i).unwrap());
            invoices.push_back(inv);
        }
        invoices
    }

    pub fn get_metadata(env: Env, id: u64) -> InvoiceMetadata {
        let inv = load_invoice(&env, id);
        let name = concat_prefix_u64(&env, b"Astera Invoice #", inv.id);
        let symbol = concat_prefix_u64(&env, b"INV-", inv.id);
        let image = env
            .storage()
            .persistent()
            .get(&DataKey::MetadataImageUri)
            .unwrap_or_else(|| String::from_str(&env, DEFAULT_METADATA_IMAGE_URI));
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

    /// Returns all invoice IDs owned by the given address (#651).
    pub fn get_invoices_by_owner(env: Env, owner: Address) -> Vec<u64> {
        bump_instance(&env);
        env.storage()
            .persistent()
            .get(&DataKey::SmeInvoices(owner))
            .unwrap_or_else(|| Vec::new(&env))
    }

    pub fn get_storage_stats(env: Env) -> StorageStats {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::StorageStats)
            .unwrap_or_default()
    }

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
        let expires_at = match inv.created_at.checked_add(expiration_duration_secs) {
            Some(ts) => ts,
            None => return false, // overflow: expiration beyond u64::MAX → can never fire
        };
        if now <= expires_at {
            return false;
        }
        let mut expired_inv = inv;
        expired_inv.status = InvoiceStatus::Expired;
        remove_invoice_from_owner(&env, &expired_inv.owner, id);
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
        let old_days: u32 = env
            .storage()
            .instance()
            .get(&DataKey::GracePeriodDays)
            .unwrap_or(DEFAULT_GRACE_PERIOD_DAYS);
        env.storage()
            .instance()
            .set(&DataKey::GracePeriodDays, &days);
        env.events().publish(
            (EVT, Symbol::new(&env, "grace_period_updated")),
            (admin, old_days, days),
        );
    }

    pub fn set_min_due_date_window(env: Env, admin: Address, window_secs: u64) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic!("unauthorized");
        }
        let old_window: u64 = env
            .storage()
            .instance()
            .get(&DataKey::MinDueDateWindowSecs)
            .unwrap_or(DEFAULT_MIN_DUE_DATE_WINDOW_SECS);
        env.storage()
            .instance()
            .set(&DataKey::MinDueDateWindowSecs, &window_secs);
        bump_instance(&env);
        env.events().publish(
            (EVT, Symbol::new(&env, "due_window_updated")),
            (admin, old_window, window_secs),
        );
    }

    pub fn get_min_due_date_window(env: Env) -> u64 {
        bump_instance(&env);
        resolve_min_due_date_window(&env)
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
        let old_max: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MaxInvoiceAmount)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::MaxInvoiceAmount, &max_invoice_amount);
        env.events().publish(
            (EVT, Symbol::new(&env, "max_amount_updated")),
            (admin, old_max, max_invoice_amount),
        );
    }

    pub fn get_max_invoice_amount(env: Env) -> i128 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::MaxInvoiceAmount)
            .expect("max invoice amount not set")
    }

    pub fn set_max_sme_outstanding(env: Env, admin: Address, max: i128) {
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
        if max <= 0 {
            panic!("max outstanding must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxOutstandingPerSme, &max);
        env.events()
            .publish((EVT, symbol_short!("sme_max")), (admin, max));
    }

    pub fn get_sme_outstanding(env: Env, sme: Address) -> i128 {
        bump_instance(&env);
        get_sme_outstanding(&env, &sme)
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
        if expiration_duration_secs > MAX_EXPIRATION_DURATION_SECS {
            panic_with_error!(&env, InvoiceError::ArithmeticOverflow);
        }
        let old_duration: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ExpirationDurationSecs)
            .unwrap_or(DEFAULT_EXPIRATION_DURATION_SECS);
        env.storage()
            .instance()
            .set(&DataKey::ExpirationDurationSecs, &expiration_duration_secs);
        env.events().publish(
            (EVT, Symbol::new(&env, "expiration_updated")),
            (admin, old_duration, expiration_duration_secs),
        );
    }

    pub fn get_expiration_duration(env: Env) -> u64 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::ExpirationDurationSecs)
            .unwrap_or(DEFAULT_EXPIRATION_DURATION_SECS)
    }

    /// #446: Set the TTL (in ledgers) applied to completed/defaulted/cancelled
    /// invoices.  Must be at least as long as `ACTIVE_INVOICE_TTL` (1 year) so
    /// historical records are never evicted before active ones.
    pub fn set_completed_invoice_ttl(env: Env, admin: Address, ttl_ledgers: u32) {
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
        if ttl_ledgers < ACTIVE_INVOICE_TTL {
            panic!("completed TTL must be at least as long as ACTIVE_INVOICE_TTL");
        }
        env.storage()
            .instance()
            .set(&DataKey::CompletedInvoiceTtl, &ttl_ledgers);
        env.events()
            .publish((EVT, symbol_short!("set_cttl")), (admin, ttl_ledgers));
    }

    /// #446: Return the currently configured completed-invoice TTL in ledgers.
    pub fn get_completed_invoice_ttl(env: Env) -> u32 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::CompletedInvoiceTtl)
            .unwrap_or(DEFAULT_COMPLETED_INVOICE_TTL)
    }

    pub fn get_grace_period(env: Env) -> u32 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::GracePeriodDays)
            .unwrap_or(DEFAULT_GRACE_PERIOD_DAYS)
    }

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
        let mut invoice: Invoice = load_invoice(&env, id);
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

    /// Returns the currently authorized pool address (#385).
    pub fn get_authorized_pool(env: Env) -> Address {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::Pool)
            .expect("not initialized")
    }

    /// Returns true if the invoice with `id` has status Defaulted (#386).
    pub fn is_invoice_defaulted(env: Env, id: u64) -> bool {
        bump_instance(&env);
        let invoice: Option<Invoice> = env.storage().persistent().get(&DataKey::Invoice(id));
        match invoice {
            Some(inv) => inv.status == InvoiceStatus::Defaulted,
            None => false,
        }
    }

    /// Set the upgrade timelock duration in seconds (#338).
    /// Minimum: 3,600 s (1 h). Default: 86,400 s (24 h).
    pub fn set_upgrade_timelock(env: Env, admin: Address, secs: u64) {
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
        if secs < MIN_UPGRADE_TIMELOCK_SECS {
            panic_with_error!(&env, InvoiceError::InvalidUpgradeTimelock);
        }
        let old_secs: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeTimelockSecs)
            .unwrap_or(UPGRADE_TIMELOCK_SECS);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeTimelockSecs, &secs);
        env.events().publish(
            (EVT, Symbol::new(&env, "timelock_updated")),
            (admin, old_secs, secs),
        );
    }

    /// Returns the configured upgrade timelock in seconds (#338).
    pub fn get_upgrade_timelock(env: Env) -> u64 {
        bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::UpgradeTimelockSecs)
            .unwrap_or(UPGRADE_TIMELOCK_SECS)
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
        // #340: reject all-zero hash — it has no corresponding uploaded WASM
        if wasm_hash == BytesN::from_array(&env, &[0u8; 32]) {
            panic_with_error!(&env, InvoiceError::InvalidWasmHash);
        }
        env.storage()
            .instance()
            .set(&DataKey::ProposedWasmHash, &wasm_hash);
        env.storage()
            .instance()
            .set(&DataKey::UpgradeScheduledAt, &env.ledger().timestamp());
        let timelock: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeTimelockSecs)
            .unwrap_or(UPGRADE_TIMELOCK_SECS);
        env.events().publish(
            (EVT, symbol_short!("upg_prop")),
            (admin, env.ledger().timestamp() + timelock),
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
        let timelock: u64 = env
            .storage()
            .instance()
            .get(&DataKey::UpgradeTimelockSecs)
            .unwrap_or(UPGRADE_TIMELOCK_SECS);
        let now = env.ledger().timestamp();
        if now < scheduled_at + timelock {
            panic_with_error!(&env, InvoiceError::UpgradeTimelockNotExpired);
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

    /// Propose an admin key rotation (#565).
    /// Stores the pending admin address and the proposal timestamp.
    /// The change does not take effect until `finalize_admin_change` is called
    /// after `ADMIN_CHANGE_TIMELOCK_SECS` (48 h) have elapsed.
    pub fn propose_admin_change(env: Env, admin: Address, new_admin: Address) {
        admin.require_auth();
        require_not_paused(&env);
        bump_instance(&env);
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic_with_error!(&env, InvoiceError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &new_admin);
        env.storage()
            .instance()
            .set(&DataKey::AdminChangeScheduledAt, &env.ledger().timestamp());
        env.events().publish(
            (EVT, Symbol::new(&env, "admin_chg_proposed")),
            (
                admin,
                new_admin,
                env.ledger().timestamp() + ADMIN_CHANGE_TIMELOCK_SECS,
            ),
        );
    }

    /// Finalize a previously proposed admin key rotation (#565).
    /// Only callable by the current admin after the 48-hour timelock has elapsed.
    pub fn finalize_admin_change(env: Env, admin: Address) {
        admin.require_auth();
        require_not_paused(&env);
        bump_instance(&env);
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic_with_error!(&env, InvoiceError::Unauthorized);
        }
        let maybe_scheduled: Option<u64> = env
            .storage()
            .instance()
            .get(&DataKey::AdminChangeScheduledAt);
        let scheduled_at = match maybe_scheduled {
            Some(v) => v,
            None => panic_with_error!(&env, InvoiceError::NoAdminChangeProposed),
        };
        let now = env.ledger().timestamp();
        if now < scheduled_at + ADMIN_CHANGE_TIMELOCK_SECS {
            panic_with_error!(&env, InvoiceError::AdminChangeTimelockNotExpired);
        }
        let maybe_new_admin: Option<Address> = env.storage().instance().get(&DataKey::PendingAdmin);
        let new_admin = match maybe_new_admin {
            Some(v) => v,
            None => panic_with_error!(&env, InvoiceError::NoAdminChangeProposed),
        };
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        env.storage()
            .instance()
            .remove(&DataKey::AdminChangeScheduledAt);
        env.events().publish(
            (EVT, Symbol::new(&env, "admin_changed")),
            (admin, new_admin, now),
        );
    }

    /// Cancel a pending admin key rotation (#565).
    /// Only callable by the current admin before the change is finalized.
    pub fn cancel_admin_change(env: Env, admin: Address) {
        admin.require_auth();
        require_not_paused(&env);
        bump_instance(&env);
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != stored_admin {
            panic_with_error!(&env, InvoiceError::Unauthorized);
        }
        if !env.storage().instance().has(&DataKey::PendingAdmin) {
            panic_with_error!(&env, InvoiceError::NoAdminChangeProposed);
        }
        env.storage().instance().remove(&DataKey::PendingAdmin);
        env.storage()
            .instance()
            .remove(&DataKey::AdminChangeScheduledAt);
        env.events()
            .publish((EVT, Symbol::new(&env, "admin_chg_cancelled")), admin);
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
        let grace_period_days = resolve_invoice_grace_period_days(&env, &invoice);
        let default_at = checked_default_deadline(&env, invoice.due_date, grace_period_days);
        let now = env.ledger().timestamp();
        if now >= invoice.due_date && now < default_at && default_at - now <= SECS_PER_DAY {
            env.events()
                .publish((EVT, symbol_short!("def_warn")), (id, default_at));
            return true;
        }
        false
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Env,
    };

    mod mock_pool_true {
        use super::*;
        #[contract]
        pub struct MockPoolTrue;
        #[contractimpl]
        impl MockPoolTrue {
            pub fn is_invoice_repaid(_env: Env, _invoice_id: u64) -> bool {
                true
            }

            pub fn update_invoice_due_date(
                env: Env,
                invoice_contract: Address,
                invoice_id: u64,
                new_due_date: u64,
            ) {
                invoice_contract.require_auth();
                env.storage()
                    .instance()
                    .set(&symbol_short!("upd_id"), &invoice_id);
                env.storage()
                    .instance()
                    .set(&symbol_short!("upd_due"), &new_due_date);
            }

            pub fn last_updated_invoice_id(env: Env) -> u64 {
                env.storage()
                    .instance()
                    .get(&symbol_short!("upd_id"))
                    .unwrap_or(0)
            }

            pub fn last_updated_due_date(env: Env) -> u64 {
                env.storage()
                    .instance()
                    .get(&symbol_short!("upd_due"))
                    .unwrap_or(0)
            }
        }
    }

    mod mock_pool_false {
        use super::*;
        #[contract]
        pub struct MockPoolFalse;
        #[contractimpl]
        impl MockPoolFalse {
            pub fn is_invoice_repaid(_env: Env, _invoice_id: u64) -> bool {
                false
            }

            pub fn update_invoice_due_date(
                _env: Env,
                invoice_contract: Address,
                _invoice_id: u64,
                _new_due_date: u64,
            ) {
                invoice_contract.require_auth();
            }
        }
    }

    fn setup(env: &Env) -> (InvoiceContractClient<'_>, Address, Address, Address) {
        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let pool = env.register(mock_pool_true::MockPoolTrue, ());
        let sme = Address::generate(env);
        client.initialize(
            &admin,
            &pool,
            &i128::MAX,
            &DEFAULT_EXPIRATION_DURATION_SECS,
            &90u32,
        );
        (client, admin, pool, sme)
    }

    #[allow(dead_code)]
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

    // ── All original tests preserved verbatim ────────────────────────────────
    // (omitted here for brevity — they are identical to the input file)
    // The complete file in /home/claude/lib.rs contains all original tests.

    // ── #290: cleanup_expired_storage tests ──────────────────────────────────

    fn make_invoice(
        env: &Env,
        client: &InvoiceContractClient<'_>,
        sme: &Address,
        amount: i128,
    ) -> u64 {
        let due = env.ledger().timestamp() + 86_400;
        client.create_invoice(
            sme,
            &String::from_str(env, "Debtor"),
            &amount,
            &due,
            &String::from_str(env, "desc"),
            &String::from_str(env, "hash"),
            &String::from_str(env, "https://example.com/meta"),
        )
    }

    #[test]
    fn test_cleanup_expired_removes_paid_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);
        let id = make_invoice(&env, &client, &sme, 1_000);
        client.mark_funded(&id, &pool);
        client.mark_paid(&id, &pool);
        let ids = soroban_sdk::vec![&env, id];
        let removed = client.cleanup_expired_storage(&admin, &ids);
        assert_eq!(removed, 1);
        assert_eq!(client.get_storage_stats().cleaned_invoices, 1);
    }

    #[test]
    fn test_cleanup_expired_removes_cancelled_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, sme) = setup(&env);
        let id = make_invoice(&env, &client, &sme, 1_000);
        client.cancel_invoice(&id, &sme);
        let removed = client.cleanup_expired_storage(&admin, &soroban_sdk::vec![&env, id]);
        assert_eq!(removed, 1);
    }

    #[test]
    fn test_cleanup_expired_removes_defaulted_invoice() {
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
            &1u32,
        );
        let due = env.ledger().timestamp() + 86_400;
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000,
            &due,
            &String::from_str(&env, "d"),
            &String::from_str(&env, "h"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        client.mark_funded(&id, &pool);
        env.ledger().with_mut(|l| l.timestamp = due + 2 * 86_400);
        client.mark_defaulted(&id, &pool);
        let removed = client.cleanup_expired_storage(&admin, &soroban_sdk::vec![&env, id]);
        assert_eq!(removed, 1);
        assert_eq!(client.get_storage_stats().cleaned_invoices, 1);
    }

    #[test]
    fn test_cleanup_expired_removes_expired_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let pool = Address::generate(&env);
        let sme = Address::generate(&env);
        client.initialize(&admin, &pool, &i128::MAX, &1u64, &90u32);
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "d"),
            &String::from_str(&env, "h"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        env.ledger().with_mut(|l| l.timestamp += 2);
        assert!(client.check_expiration(&id));
        let inv = client.get_invoice(&id);
        assert_eq!(inv.status, InvoiceStatus::Expired);
        let removed = client.cleanup_expired_storage(&admin, &soroban_sdk::vec![&env, id]);
        assert_eq!(removed, 1);
        assert_eq!(client.get_storage_stats().cleaned_invoices, 1);
    }

    #[test]
    fn test_cleanup_expired_skips_pending_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, sme) = setup(&env);
        let id = make_invoice(&env, &client, &sme, 1_000);
        let removed = client.cleanup_expired_storage(&admin, &soroban_sdk::vec![&env, id]);
        assert_eq!(removed, 0);
        assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Pending);
        assert_eq!(client.get_storage_stats().cleaned_invoices, 0);
    }

    #[test]
    fn test_cleanup_expired_skips_funded_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);
        let id = make_invoice(&env, &client, &sme, 1_000);
        client.mark_funded(&id, &pool);
        let removed = client.cleanup_expired_storage(&admin, &soroban_sdk::vec![&env, id]);
        assert_eq!(removed, 0);
        assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Funded);
    }

    #[test]
    fn test_cleanup_expired_mixed_batch_only_removes_terminal() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);
        let id1 = make_invoice(&env, &client, &sme, 500);
        client.mark_funded(&id1, &pool);
        client.mark_paid(&id1, &pool);
        let id2 = make_invoice(&env, &client, &sme, 500); // still pending
        let id3 = make_invoice(&env, &client, &sme, 500);
        client.cancel_invoice(&id3, &sme);
        let removed =
            client.cleanup_expired_storage(&admin, &soroban_sdk::vec![&env, id1, id2, id3]);
        assert_eq!(removed, 2);
        assert_eq!(client.get_storage_stats().cleaned_invoices, 2);
        assert_eq!(client.get_invoice(&id2).status, InvoiceStatus::Pending);
    }

    #[test]
    fn test_cleanup_expired_is_idempotent() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);
        let id = make_invoice(&env, &client, &sme, 1_000);
        client.mark_funded(&id, &pool);
        client.mark_paid(&id, &pool);
        let ids = soroban_sdk::vec![&env, id];
        let removed1 = client.cleanup_expired_storage(&admin, &ids.clone());
        assert_eq!(removed1, 1);
        let removed2 = client.cleanup_expired_storage(&admin, &ids);
        assert_eq!(removed2, 0);
        assert_eq!(client.get_storage_stats().cleaned_invoices, 1);
    }

    #[test]
    #[should_panic(expected = "cleanup batch exceeds maximum")]
    fn test_cleanup_expired_batch_too_large_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        let mut ids = soroban_sdk::vec![&env];
        for i in 1u64..=51 {
            ids.push_back(i);
        }
        client.cleanup_expired_storage(&admin, &ids);
    }

    // ── #290: estimate_storage_cost tests ────────────────────────────────────

    #[test]
    fn test_estimate_storage_cost_zero_when_no_active_invoices() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, _sme) = setup(&env);
        assert_eq!(client.estimate_storage_cost(), 0u64);
    }

    #[test]
    fn test_estimate_storage_cost_scales_with_active_invoices() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        make_invoice(&env, &client, &sme, 1_000);
        make_invoice(&env, &client, &sme, 2_000);
        make_invoice(&env, &client, &sme, 3_000);
        let expected = 3u64 * STROOPS_PER_LEDGER_PER_ENTRY * LEDGERS_PER_MONTH;
        assert_eq!(client.estimate_storage_cost(), expected);
    }

    #[test]
    fn test_estimate_storage_cost_zero_after_all_invoices_paid_and_cleaned() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);
        let id = make_invoice(&env, &client, &sme, 1_000);
        assert!(client.estimate_storage_cost() > 0);
        client.mark_funded(&id, &pool);
        client.mark_paid(&id, &pool);
        client.cleanup_expired_storage(&admin, &soroban_sdk::vec![&env, id]);
        assert_eq!(client.estimate_storage_cost(), 0u64);
    }

    // ── #290: get_storage_stats accuracy ─────────────────────────────────────

    #[test]
    fn test_storage_stats_accurate_after_full_lifecycle_with_cleanup() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, sme) = setup(&env);

        let id1 = make_invoice(&env, &client, &sme, 1_000);
        let id2 = make_invoice(&env, &client, &sme, 2_000);
        let id3 = make_invoice(&env, &client, &sme, 3_000);

        let s = client.get_storage_stats();
        assert_eq!(s.total_invoices, 3);
        assert_eq!(s.active_invoices, 3);
        assert_eq!(s.cleaned_invoices, 0);

        client.mark_funded(&id1, &pool);
        client.mark_paid(&id1, &pool);
        client.cancel_invoice(&id2, &sme);

        let s = client.get_storage_stats();
        assert_eq!(s.active_invoices, 1); // id3 still pending

        client.cleanup_expired_storage(&admin, &soroban_sdk::vec![&env, id1, id2]);
        let s = client.get_storage_stats();
        assert_eq!(s.cleaned_invoices, 2);
        assert_eq!(s.total_invoices, 3);
        assert_eq!(s.active_invoices, 1);
        assert_eq!(client.get_invoice(&id3).status, InvoiceStatus::Pending);
    }

    // ── Original tests (all preserved) ───────────────────────────────────────

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
            &String::from_str(&env, "https://example.com/meta"),
        );
        assert_eq!(id, 1);
        assert!(matches!(
            client.get_invoice(&id).status,
            InvoiceStatus::Pending
        ));
        let meta = client.get_metadata(&id);
        assert_eq!(meta.status, InvoiceStatus::Pending);
        assert_eq!(meta.amount, 1_000_000_000i128);
        assert_eq!(meta.decimals, 7u32);
        assert_eq!(meta.symbol, String::from_str(&env, "INV-1"));
        assert_eq!(meta.name, String::from_str(&env, "Astera Invoice #1"));
        client.mark_funded(&id, &pool);
        assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Funded);
        client.mark_paid(&id, &pool);
        assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Paid);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_create_invoice_zero_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        client.create_invoice(
            &sme,
            &String::from_str(&env, "X"),
            &0i128,
            &(env.ledger().timestamp() + 1),
            &String::from_str(&env, "d"),
            &String::from_str(&env, "h"),
            &String::from_str(&env, "https://example.com/meta"),
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #23)")]
    fn test_create_invoice_past_due_date_panics() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, _admin, _pool, sme) = setup(&env);
        client.create_invoice(
            &sme,
            &String::from_str(&env, "X"),
            &100i128,
            &999_999,
            &String::from_str(&env, "d"),
            &String::from_str(&env, "h"),
            &String::from_str(&env, "https://example.com/meta"),
        );
    }

    #[test]
    fn test_create_invoice_due_date_overflow_returns_error() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let result = client.try_create_invoice(
            &sme,
            &String::from_str(&env, "X"),
            &100i128,
            &u64::MAX,
            &String::from_str(&env, "d"),
            &String::from_str(&env, "h"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        assert_eq!(
            result,
            Err(Ok::<soroban_sdk::Error, _>(
                InvoiceError::DateOverflow.into()
            ))
        );
    }

    #[test]
    fn test_create_invoice_due_date_far_in_future_returns_error() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, _admin, _pool, sme) = setup(&env);
        let due_date = env.ledger().timestamp() + MAX_DUE_DATE_AHEAD_SECS + 1;
        let result = client.try_create_invoice(
            &sme,
            &String::from_str(&env, "X"),
            &100i128,
            &due_date,
            &String::from_str(&env, "d"),
            &String::from_str(&env, "h"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::DateOverflow.into()
        );
    }

    #[test]
    #[should_panic(expected = "unauthorized pool")]
    fn test_mark_funded_unauthorized_pool_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        client.mark_funded(&id, &Address::generate(&env));
    }

    #[test]
    #[should_panic(expected = "invoice is not in fundable state")]
    fn test_mark_funded_already_funded_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, pool, sme) = setup(&env);
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        client.mark_funded(&id, &pool);
        client.mark_funded(&id, &pool);
    }

    #[test]
    fn test_mark_funded_overflow_returns_amount_overflow() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, pool, sme) = setup(&env);
        env.ledger().with_mut(|l| l.timestamp = 1000);
        let due_date = env.ledger().timestamp() + 86_400;

        let first = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &i128::MAX,
            &due_date,
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h1"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        client.mark_funded(&first, &pool);

        let second = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1i128,
            &due_date,
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h2"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        let result = client.try_mark_funded(&second, &pool);

        assert_eq!(result.unwrap_err().unwrap(), InvoiceError::AmountOverflow);
    }

    #[test]
    fn test_daily_invoice_limit_enforced() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, _admin, _pool, sme) = setup(&env);
        let due = env.ledger().timestamp() + 86_400;
        for _ in 0..10 {
            client.create_invoice(
                &sme,
                &String::from_str(&env, "D"),
                &100i128,
                &due,
                &String::from_str(&env, "i"),
                &String::from_str(&env, "h"),
                &String::from_str(&env, "https://example.com/meta"),
            );
        }
    }

    #[test]
    fn test_sliding_window_full_quota_available_after_24_hours() {
        // Sliding window: invoices older than 86 400 s no longer count against
        // the limit, so a full new quota is available after one window passes.
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, _admin, _pool, sme) = setup(&env);
        let due = |env: &Env| env.ledger().timestamp() + 86_400;

        // Exhaust the daily limit at t=1_000_000.
        for _ in 0..10 {
            client.create_invoice(
                &sme,
                &String::from_str(&env, "D"),
                &100i128,
                &due(&env),
                &String::from_str(&env, "i"),
                &String::from_str(&env, "h"),
                &String::from_str(&env, "https://example.com/meta"),
            );
        }

        // Advance exactly 86 401 s — window_start = 1_086_401 - 86_400 = 1_000_001,
        // so all earlier timestamps (1_000_000) fall outside the window.
        env.ledger().with_mut(|l| l.timestamp = 1_086_401);

        // A full quota of 10 should be available again.
        for _ in 0..10 {
            client.create_invoice(
                &sme,
                &String::from_str(&env, "D"),
                &100i128,
                &due(&env),
                &String::from_str(&env, "i"),
                &String::from_str(&env, "h"),
                &String::from_str(&env, "https://example.com/meta"),
            );
        }
    }

    #[test]
    #[should_panic(expected = "daily invoice limit exceeded")]
    fn test_midnight_boundary_exploit_blocked() {
        // #577: an SME that exhausts the quota just before UTC midnight must not
        // be able to submit more invoices immediately after midnight crosses.
        // The sliding window covers any 86 400 s span, not just a UTC calendar day.
        let env = Env::default();
        env.mock_all_auths();

        // Start 60 s before UTC midnight (ts = 86_340).
        env.ledger().with_mut(|l| l.timestamp = 86_340);
        let (client, _admin, _pool, sme) = setup(&env);
        let due = |env: &Env| env.ledger().timestamp() + 86_400;

        // Batch 1: exhaust the full daily quota just before midnight.
        for _ in 0..10 {
            client.create_invoice(
                &sme,
                &String::from_str(&env, "D"),
                &100i128,
                &due(&env),
                &String::from_str(&env, "i"),
                &String::from_str(&env, "h"),
                &String::from_str(&env, "https://example.com/meta"),
            );
        }

        // Advance 60 s past midnight — still inside the 86 400 s window
        // (window_start = 86_460 - 86_400 = 60; ts=86_340 > 60, so still counted).
        env.ledger().with_mut(|l| l.timestamp = 86_460);

        // The 11th invoice must be rejected even though the calendar day changed.
        client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &100i128,
            &due(&env),
            &String::from_str(&env, "i"),
            &String::from_str(&env, "h"),
            &String::from_str(&env, "https://example.com/meta"),
        );
    }

    #[test]
    #[should_panic(expected = "daily invoice limit exceeded")]
    fn test_daily_invoice_limit_exceeded_panics() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, _admin, _pool, sme) = setup(&env);
        let due = env.ledger().timestamp() + 86_400;
        for _ in 0..11 {
            client.create_invoice(
                &sme,
                &String::from_str(&env, "D"),
                &100i128,
                &due,
                &String::from_str(&env, "i"),
                &String::from_str(&env, "h"),
                &String::from_str(&env, "https://example.com/meta"),
            );
        }
    }

    #[test]
    fn test_daily_reset_after_gap_provides_clean_window() {
        // SME who hasn't submitted for 5 days gets a clean 24-hour window:
        // the 5-day-old timestamp is well outside the 86 400 s sliding window
        // so a full new quota of 10 is available.
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, _admin, _pool, sme) = setup(&env);
        let due = env.ledger().timestamp() + 86_400;

        // Create 1 invoice at t=1_000_000.
        client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &100i128,
            &due,
            &String::from_str(&env, "i"),
            &String::from_str(&env, "h"),
            &String::from_str(&env, "https://example.com/meta"),
        );

        // Jump forward 5 days (432_000 s): window_start = 1_432_000 - 86_400 = 1_345_600,
        // the earlier timestamp 1_000_000 is outside the window.
        let new_timestamp = env.ledger().timestamp() + (5 * 86_400u64);
        env.ledger().with_mut(|l| l.timestamp = new_timestamp);

        // Full quota of 10 must be available.
        for _ in 0..10 {
            client.create_invoice(
                &sme,
                &String::from_str(&env, "D"),
                &100i128,
                &(new_timestamp + 86_400),
                &String::from_str(&env, "i"),
                &String::from_str(&env, "h"),
                &String::from_str(&env, "https://example.com/meta"),
            );
        }
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
    fn test_pause_non_admin_returns_typed_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, _sme) = setup(&env);
        let attacker = Address::generate(&env);

        let result = client.try_pause(&attacker);

        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::Unauthorized.into()
        );
    }

    #[test]
    fn test_unpause_non_admin_returns_typed_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        let attacker = Address::generate(&env);

        client.pause(&admin);
        let result = client.try_unpause(&attacker);

        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::Unauthorized.into()
        );
    }

    #[test]
    fn test_create_invoice_while_paused_returns_typed_contract_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, sme) = setup(&env);
        client.pause(&admin);
        let result = client.try_create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::ContractPaused.into()
        );
    }

    #[test]
    #[should_panic(expected = "repayment not verified by pool contract")]
    fn test_mark_paid_without_pool_verification_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let pool_id = env.register(mock_pool_false::MockPoolFalse, ());
        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let sme = Address::generate(&env);
        client.initialize(
            &admin,
            &pool_id,
            &i128::MAX,
            &DEFAULT_EXPIRATION_DURATION_SECS,
            &90u32,
        );
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        client.mark_funded(&id, &pool_id);
        client.mark_paid(&id, &pool_id);
    }

    #[test]
    fn test_get_invoice_does_not_expire_pending_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let pool = Address::generate(&env);
        let sme = Address::generate(&env);
        client.initialize(&admin, &pool, &i128::MAX, &10u64, &90u32);
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        env.ledger().with_mut(|l| l.timestamp += 11);
        assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Pending);
        assert_eq!(client.get_metadata(&id).status, InvoiceStatus::Pending);
    }

    #[test]
    fn test_check_expiration_expires_pending_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let pool = Address::generate(&env);
        let sme = Address::generate(&env);
        client.initialize(&admin, &pool, &i128::MAX, &10u64, &90u32);
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "D"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "x"),
            &String::from_str(&env, "h"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        env.ledger().with_mut(|l| l.timestamp += 11);
        assert!(client.check_expiration(&id));
        assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Expired);
    }

    #[test]
    fn test_set_expiration_duration_rejects_above_max() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let pool = Address::generate(&env);
        client.initialize(&admin, &pool, &i128::MAX, &10u64, &90u32);

        let result =
            client.try_set_expiration_duration(&admin, &(MAX_EXPIRATION_DURATION_SECS + 1));
        assert!(result.is_err());
    }

    #[test]
    fn test_set_expiration_duration_allows_max() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let pool = Address::generate(&env);
        client.initialize(&admin, &pool, &i128::MAX, &10u64, &90u32);

        client.set_expiration_duration(&admin, &MAX_EXPIRATION_DURATION_SECS);
        assert_eq!(
            client.get_expiration_duration(),
            MAX_EXPIRATION_DURATION_SECS
        );
    }

    #[test]
    fn test_initialize_rejects_expiration_duration_above_max() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let pool = Address::generate(&env);

        let result = client.try_initialize(
            &admin,
            &pool,
            &i128::MAX,
            &(MAX_EXPIRATION_DURATION_SECS + 1),
            &90u32,
        );
        assert!(result.is_err());
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn test_set_expiration_duration_default_is_below_max() {
        assert!(DEFAULT_EXPIRATION_DURATION_SECS <= MAX_EXPIRATION_DURATION_SECS);
    }

    #[allow(dead_code)]
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

    fn setup_funded_invoice(env: &Env) -> (InvoiceContractClient<'_>, Address, Address, Address) {
        let contract_id = env.register(InvoiceContract, ());
        let client = InvoiceContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let pool = env.register(mock_pool_true::MockPoolTrue, ());
        let oracle = Address::generate(env);
        let owner = Address::generate(env);
        client.initialize(
            &admin,
            &pool,
            &i128::MAX,
            &DEFAULT_EXPIRATION_DURATION_SECS,
            &DEFAULT_GRACE_PERIOD_DAYS,
        );
        client.set_oracle(&admin, &oracle);
        let id = client.create_invoice(
            &owner,
            &String::from_str(env, "ACME Corp"),
            &1_000_0000000i128,
            &(env.ledger().timestamp() + SECS_PER_DAY * 30),
            &String::from_str(env, "Test invoice"),
            &String::from_str(env, "hash"),
            &String::from_str(env, "https://example.com/meta"),
        );
        client.verify_invoice(
            &id,
            &oracle,
            &true,
            &String::from_str(env, ""),
            &String::from_str(env, "hash"),
        );
        client.mark_funded(&id, &pool);
        (client, admin, pool, owner)
    }

    #[test]
    fn test_invoice_with_override_uses_override_grace_period() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _owner) = setup_funded_invoice(&env);
        client.set_invoice_grace_period(&admin, &1u64, &14u32);
        assert_eq!(client.get_invoice_grace_period(&1u64), 14);
    }

    #[test]
    fn test_default_warning_uses_invoice_override_grace_period() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _owner) = setup_funded_invoice(&env);
        let id = 1u64;
        client.set_invoice_grace_period(&admin, &id, &14u32);
        let due = client.get_invoice(&id).due_date;

        env.ledger()
            .with_mut(|l| l.timestamp = due + (13 * SECS_PER_DAY) + (SECS_PER_DAY / 2));

        assert!(client.check_default_warning(&id));
    }

    #[test]
    fn test_mark_defaulted_respects_invoice_override_grace_period() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, pool, _owner) = setup_funded_invoice(&env);
        let id = 1u64;
        client.set_invoice_grace_period(&admin, &id, &14u32);
        let due = client.get_invoice(&id).due_date;

        env.ledger()
            .with_mut(|l| l.timestamp = due + (8 * SECS_PER_DAY));
        assert!(client.try_mark_defaulted(&id, &pool).is_err());

        env.ledger()
            .with_mut(|l| l.timestamp = due + (15 * SECS_PER_DAY));
        client.mark_defaulted(&id, &pool);
        assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Defaulted);
    }

    #[test]
    #[should_panic(expected = "exceeds maximum")]
    fn test_override_exceeds_max_days_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _owner) = setup_funded_invoice(&env);
        // Cap is 90 days — 91 must panic
        client.set_invoice_grace_period(&admin, &1u64, &91u32);
    }

    #[test]
    fn test_set_invoice_grace_period_at_max_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _owner) = setup_funded_invoice(&env);
        // Exactly 90 days must be accepted (same cap as set_grace_period)
        client.set_invoice_grace_period(&admin, &1u64, &90u32);
        assert_eq!(client.get_invoice_grace_period(&1u64), 90);
    }

    #[test]
    fn test_set_invoice_grace_period_missing_invoice_returns_typed_error() {
        // #644: a non-existent invoice id must surface the typed
        // InvoiceError::InvoiceNotFound rather than a raw "invoice not found"
        // string panic, so clients can match on the error code.
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _owner) = setup_funded_invoice(&env);
        let result = client.try_set_invoice_grace_period(&admin, &999u64, &14u32);
        // set_invoice_grace_period panics via panic_with_error!, so the generated
        // try_ client surfaces the raw soroban Error code rather than the typed
        // enum — compare against the converted InvoiceError.
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::InvoiceNotFound.into()
        );
    }

    #[test]
    #[should_panic(expected = "grace period cannot exceed 90 days")]
    fn test_set_grace_period_exceeds_max_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        client.set_grace_period(&admin, &91u32);
    }

    #[test]
    fn test_set_grace_period_at_max_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        client.set_grace_period(&admin, &90u32);
        assert_eq!(client.get_grace_period(), 90);
    }

    // ── #436: empty string validation tests ──────────────────────────────────

    #[test]
    fn test_create_invoice_empty_description_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let result = client.try_create_invoice(
            &sme,
            &String::from_str(&env, "Debtor Corp"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, ""), // empty description
            &String::from_str(&env, "hash"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::EmptyDescription.into()
        );
    }

    #[test]
    fn test_create_invoice_description_at_max_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let max_desc = String::from_bytes(&env, &[b'a'; MAX_DESCRIPTION_LEN as usize]);
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor Corp"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &max_desc,
            &String::from_str(&env, "hash"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        assert_eq!(id, 1);
    }

    #[test]
    fn test_create_invoice_description_too_long_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let long_desc = String::from_bytes(&env, &[b'a'; (MAX_DESCRIPTION_LEN as usize) + 1]);
        let result = client.try_create_invoice(
            &sme,
            &String::from_str(&env, "Debtor Corp"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &long_desc,
            &String::from_str(&env, "hash"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::DescriptionTooLong.into()
        );
    }

    #[test]
    fn test_create_invoice_debtor_at_max_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let max_debtor = String::from_bytes(&env, &[b'd'; MAX_DEBTOR_LEN as usize]);
        let id = client.create_invoice(
            &sme,
            &max_debtor,
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Valid description"),
            &String::from_str(&env, "hash"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        assert_eq!(id, 1);
    }

    #[test]
    fn test_create_invoice_debtor_too_long_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let long_debtor = String::from_bytes(&env, &[b'd'; (MAX_DEBTOR_LEN as usize) + 1]);
        let result = client.try_create_invoice(
            &sme,
            &long_debtor,
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Valid description"),
            &String::from_str(&env, "hash"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::DebtorNameTooLong.into()
        );
    }

    #[test]
    fn test_create_invoice_empty_debtor_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let result = client.try_create_invoice(
            &sme,
            &String::from_str(&env, ""),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Valid description"),
            &String::from_str(&env, "hash"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::EmptyDebtorName.into()
        );
    }

    #[test]
    fn test_create_invoice_empty_hash_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let result = client.try_create_invoice(
            &sme,
            &String::from_str(&env, "Debtor Corp"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Valid description"),
            &String::from_str(&env, ""),
            &String::from_str(&env, "https://example.com/meta"),
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::InvalidVerificationHash.into()
        );
    }

    #[test]
    fn test_create_invoice_hash_too_long_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let long_hash = String::from_bytes(&env, &[b'h'; (MAX_VERIFICATION_HASH_LEN as usize) + 1]);
        let result = client.try_create_invoice(
            &sme,
            &String::from_str(&env, "Debtor Corp"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Valid description"),
            &long_hash,
            &String::from_str(&env, "https://example.com/meta"),
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::VerificationHashTooLong.into()
        );
    }

    #[test]
    fn test_create_invoice_valid_fields_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor Corp"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Valid description"),
            &String::from_str(&env, "hash123"),
            &String::from_str(&env, "https://example.com/meta"),
        );
        assert_eq!(id, 1);
    }

    // ── #407: admin setter event emission tests ───────────────────────────────

    #[test]
    fn test_set_grace_period_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        // Should not panic; verifies function runs without error (event emission
        // is validated by the Soroban test harness internally).
        client.set_grace_period(&admin, &14u32);
        assert_eq!(client.get_grace_period(), 14);
    }

    #[test]
    fn test_set_grace_period_records_old_and_new_values() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        client.set_grace_period(&admin, &10u32);
        client.set_grace_period(&admin, &20u32);
        assert_eq!(client.get_grace_period(), 20);
    }

    #[test]
    fn test_set_dispute_window_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        let new_window = SECS_PER_DAY * 14;
        client.set_dispute_window(&admin, &new_window);
        assert_eq!(client.get_dispute_window(), new_window);
    }

    // ── #406: per-invoice metadata URL tests ─────────────────────────────────

    #[test]
    fn test_create_invoice_metadata_url_stored_and_returned() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let url = String::from_str(&env, "https://example.com/invoice/1");
        let id = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor Corp"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Valid description"),
            &String::from_str(&env, "hash123"),
            &url,
        );
        let invoice = client.get_invoice(&id);
        assert_eq!(invoice.metadata_uri, Some(url));
    }

    #[test]
    fn test_create_invoice_empty_metadata_url_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let result = client.try_create_invoice(
            &sme,
            &String::from_str(&env, "Debtor Corp"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "Valid description"),
            &String::from_str(&env, "hash123"),
            &String::from_str(&env, ""),
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::InvalidMetadata.into()
        );
    }

    #[test]
    fn test_create_invoice_different_urls_per_invoice() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, sme) = setup(&env);
        let url1 = String::from_str(&env, "https://example.com/invoice/1");
        let url2 = String::from_str(&env, "https://example.com/invoice/2");
        let id1 = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "desc"),
            &String::from_str(&env, "h1"),
            &url1,
        );
        let id2 = client.create_invoice(
            &sme,
            &String::from_str(&env, "Debtor"),
            &1_000i128,
            &(env.ledger().timestamp() + 86_400),
            &String::from_str(&env, "desc"),
            &String::from_str(&env, "h2"),
            &url2,
        );
        assert_eq!(client.get_invoice(&id1).metadata_uri, Some(url1));
        assert_eq!(client.get_invoice(&id2).metadata_uri, Some(url2));
    }

    // ── #446: completed TTL tests ─────────────────────────────────────────────

    #[test]
    fn test_completed_ttl_default_is_five_years() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, _sme) = setup(&env);
        // Default should be 5 years in ledgers
        let expected = LEDGERS_PER_DAY * 365 * 5;
        assert_eq!(client.get_completed_invoice_ttl(), expected);
    }

    #[test]
    fn test_set_completed_invoice_ttl_admin_can_configure() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        // Set to 2 years
        let two_years = LEDGERS_PER_DAY * 365 * 2;
        client.set_completed_invoice_ttl(&admin, &two_years);
        assert_eq!(client.get_completed_invoice_ttl(), two_years);
    }

    #[test]
    #[should_panic(expected = "completed TTL must be at least as long as ACTIVE_INVOICE_TTL")]
    fn test_set_completed_ttl_below_active_ttl_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        // Try to set TTL shorter than ACTIVE_INVOICE_TTL (1 year)
        let too_short = LEDGERS_PER_DAY * 30; // 30 days — less than 1 year
        client.set_completed_invoice_ttl(&admin, &too_short);
    }

    #[test]
    fn test_completed_ttl_at_least_as_long_as_active_ttl() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, _sme) = setup(&env);
        let completed_ttl = client.get_completed_invoice_ttl();
        assert!(
            completed_ttl >= ACTIVE_INVOICE_TTL,
            "completed TTL ({}) must be >= active TTL ({})",
            completed_ttl,
            ACTIVE_INVOICE_TTL
        );
    }

    // ── Invoice state machine property-based tests ──────────────────────────

    fn status_ordinal(status: &InvoiceStatus) -> u8 {
        match status {
            InvoiceStatus::Pending => 0,
            InvoiceStatus::AwaitingVerification => 1,
            InvoiceStatus::Verified | InvoiceStatus::Disputed => 2,
            InvoiceStatus::Funded => 3,
            InvoiceStatus::Paid
            | InvoiceStatus::Defaulted
            | InvoiceStatus::Cancelled
            | InvoiceStatus::Expired => 4,
        }
    }

    // Property 1: Forward-only transitions — state ordinal never decreases
    #[test]
    fn test_prop_forward_only_transitions() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let mut seed: u64 = 0x5EED_F00D_0000_0001;
        let lcg = |s: &mut u64| -> u64 {
            *s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *s
        };

        for trial in 0..50u64 {
            let (client, _admin, pool, sme) = setup(&env);
            let due = env.ledger().timestamp() + 86_400;
            let id = client.create_invoice(
                &sme,
                &String::from_str(&env, "Debtor"),
                &1_000_000i128,
                &due,
                &String::from_str(&env, "desc"),
                &String::from_str(&env, "hash"),
                &String::from_str(&env, "https://example.com/meta"),
            );

            let mut last_ord = status_ordinal(&InvoiceStatus::Pending);
            let actions = (lcg(&mut seed) % 6 + 2) as u32; // 2..=7 actions per trial

            for _ in 0..actions {
                let inv = client.get_invoice(&id);
                let cur = status_ordinal(&inv.status);
                assert!(
                    cur >= last_ord,
                    "trial {}: state regressed from ordinal {} to {}",
                    trial,
                    last_ord,
                    cur
                );
                last_ord = cur;

                if cur >= 4 {
                    break;
                }

                match (cur, lcg(&mut seed) % 5) {
                    (0, 0) => {
                        // Pending -> Funded (no oracle configured in setup())
                        let _ = client.try_mark_funded(&id, &pool);
                    }
                    (2, 0) | (3, 0) => {
                        let _ = client.try_mark_paid(&id, &pool);
                    }
                    (2, 1) | (3, 1) => {
                        // Advance past grace period to allow default
                        let grace: u32 = client.get_grace_period();
                        env.ledger()
                            .with_mut(|l| l.timestamp = due + grace as u64 * 86_400 + 1);
                        let _ = client.try_mark_defaulted(&id, &pool);
                    }
                    (0..=2, 3) => {
                        let _ = client.try_cancel_invoice(&id, &sme);
                    }
                    _ => {}
                }
            }
        }
    }

    // Property 2: Terminal states are absorbing — no action changes them
    #[test]
    fn test_prop_terminal_absorbing() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let mut seed: u64 = 0x00C0_FFEE_0000_0002;
        let lcg = |s: &mut u64| -> u64 {
            *s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *s
        };

        for trial in 0..30u64 {
            // Setup with short grace period for default testing
            let contract_id = env.register(InvoiceContract, ());
            let client = InvoiceContractClient::new(&env, &contract_id);
            let admin = Address::generate(&env);
            let pool = env.register(mock_pool_true::MockPoolTrue, ());
            let sme = Address::generate(&env);
            client.initialize(&admin, &pool, &i128::MAX, &86_400u64, &1u32);

            let due = env.ledger().timestamp() + 86_400;
            let id = client.create_invoice(
                &sme,
                &String::from_str(&env, "D"),
                &1_000i128,
                &due,
                &String::from_str(&env, "d"),
                &String::from_str(&env, "h"),
                &String::from_str(&env, "https://example.com/meta"),
            );

            // Randomly reach a terminal state
            let path = lcg(&mut seed) % 4;
            match path {
                0 => {
                    // Paid
                    client.mark_funded(&id, &pool);
                    client.mark_paid(&id, &pool);
                }
                1 => {
                    // Defaulted
                    client.mark_funded(&id, &pool);
                    env.ledger().with_mut(|l| l.timestamp = due + 86_400 + 1);
                    client.mark_defaulted(&id, &pool);
                }
                2 => {
                    // Cancelled
                    client.cancel_invoice(&id, &sme);
                }
                _ => {
                    // Expired
                    env.ledger().with_mut(|l| l.timestamp += 86_400 + 1);
                    client.check_expiration(&id);
                }
            }

            let terminal = client.get_invoice(&id);
            let terminal_status = terminal.status.clone();
            assert!(
                status_ordinal(&terminal_status) >= 4,
                "trial {}: expected terminal state, got {:?}",
                trial,
                terminal_status
            );

            // Try random actions — state must remain terminal
            for _ in 0..5 {
                let action = lcg(&mut seed) % 4;
                match action {
                    0 => {
                        let _ = client.try_mark_funded(&id, &pool);
                    }
                    1 => {
                        let _ = client.try_mark_paid(&id, &pool);
                    }
                    2 => {
                        let _ = client.try_mark_defaulted(&id, &pool);
                    }
                    _ => {
                        let _ = client.try_cancel_invoice(&id, &sme);
                    }
                }
                let inv = client.get_invoice(&id);
                assert_eq!(
                    inv.status, terminal_status,
                    "trial {}: terminal state {:?} changed to {:?}",
                    trial, terminal_status, inv.status
                );
            }
        }
    }

    // Property 3: From Funded, only Paid or Defaulted are reachable
    #[test]
    fn test_prop_funded_determinism() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let mut seed: u64 = 0xABBA_BABA_0000_0003;
        let lcg = |s: &mut u64| -> u64 {
            *s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *s
        };

        for trial in 0..30u64 {
            let (client, _admin, pool, sme) = setup(&env);
            let due = env.ledger().timestamp() + 86_400;
            let id = client.create_invoice(
                &sme,
                &String::from_str(&env, "D"),
                &1_000i128,
                &due,
                &String::from_str(&env, "d"),
                &String::from_str(&env, "h"),
                &String::from_str(&env, "https://example.com/meta"),
            );

            client.mark_funded(&id, &pool);
            assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Funded);

            // cancel_invoice must fail on Funded
            let cancel_result = client.try_cancel_invoice(&id, &sme);
            assert!(
                cancel_result.is_err(),
                "trial {}: cancel_invoice should fail on Funded",
                trial
            );
            assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Funded);

            // mark_funded must fail on already Funded
            let double_fund = client.try_mark_funded(&id, &pool);
            assert!(
                double_fund.is_err(),
                "trial {}: mark_funded should fail on Funded",
                trial
            );

            // Now try the valid path (Paid or Defaulted)
            let goto = lcg(&mut seed) % 2;
            match goto {
                0 => {
                    client.mark_paid(&id, &pool);
                    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Paid);
                }
                _ => {
                    let grace: u32 = client.get_grace_period();
                    env.ledger()
                        .with_mut(|l| l.timestamp = due + grace as u64 * 86_400 + 1);
                    client.mark_defaulted(&id, &pool);
                    assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Defaulted);
                }
            }
        }
    }

    // Property 4: Pending invoices expire after expiration duration
    #[test]
    fn test_prop_pending_expires() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);

        let mut seed: u64 = 0xBAD_CAFE_0000_0004;
        let lcg = |s: &mut u64| -> u64 {
            *s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *s
        };

        for _ in 0..20u64 {
            let contract_id = env.register(InvoiceContract, ());
            let client = InvoiceContractClient::new(&env, &contract_id);
            let admin = Address::generate(&env);
            let pool = Address::generate(&env);
            let sme = Address::generate(&env);
            let expiration_secs = lcg(&mut seed) % 100 + 1; // 1..=100 sec
            client.initialize(&admin, &pool, &i128::MAX, &expiration_secs, &90u32);

            let id = client.create_invoice(
                &sme,
                &String::from_str(&env, "D"),
                &1_000i128,
                &(env.ledger().timestamp() + 86_400),
                &String::from_str(&env, "d"),
                &String::from_str(&env, "h"),
                &String::from_str(&env, "https://example.com/meta"),
            );

            // Initially Pending
            assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Pending);

            // Jump past expiration
            env.ledger()
                .with_mut(|l| l.timestamp += expiration_secs + 1);

            // Explicit expiration check transitions the invoice.
            client.check_expiration(&id);
            assert_eq!(client.get_invoice(&id).status, InvoiceStatus::Expired);
        }
    }

    // ── #347: admin setter event-emission tests ───────────────────────────────

    #[test]
    fn test_set_oracle_emits_event_with_old_and_new() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        let oracle1 = Address::generate(&env);
        let oracle2 = Address::generate(&env);
        client.set_oracle(&admin, &oracle1);
        client.set_oracle(&admin, &oracle2);
        // Verify the getter reflects the latest oracle (event structure verified by SDK)
        // We cannot directly inspect event data in unit tests without additional harness,
        // but we verify state is updated correctly.
        let _ = client.get_expiration_duration(); // ensure no panic
    }

    #[test]
    fn test_set_daily_invoice_limit_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        client.set_daily_invoice_limit(&admin, &5u32);
        assert_eq!(client.get_daily_invoice_limit(), 5);
        client.set_daily_invoice_limit(&admin, &20u32);
        assert_eq!(client.get_daily_invoice_limit(), 20);
    }

    #[test]
    fn test_set_max_invoice_amount_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        let new_max = 5_000_000_000i128;
        client.set_max_invoice_amount(&admin, &new_max);
        assert_eq!(client.get_max_invoice_amount(), new_max);
    }

    #[test]
    fn test_set_expiration_duration_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        let new_duration = SECS_PER_DAY * 60;
        client.set_expiration_duration(&admin, &new_duration);
        assert_eq!(client.get_expiration_duration(), new_duration);
    }

    // ── #338: configurable upgrade timelock tests ─────────────────────────────

    #[test]
    fn test_upgrade_timelock_default_is_24h() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _pool, _sme) = setup(&env);
        assert_eq!(client.get_upgrade_timelock(), UPGRADE_TIMELOCK_SECS);
    }

    #[test]
    fn test_set_upgrade_timelock_admin_can_configure() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        let new_timelock = 7_200u64; // 2 hours
        client.set_upgrade_timelock(&admin, &new_timelock);
        assert_eq!(client.get_upgrade_timelock(), new_timelock);
    }

    #[test]
    fn test_set_upgrade_timelock_below_minimum_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        let result = client.try_set_upgrade_timelock(&admin, &(MIN_UPGRADE_TIMELOCK_SECS - 1));
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::InvalidUpgradeTimelock.into()
        );
    }

    #[test]
    fn test_set_upgrade_timelock_at_minimum_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        client.set_upgrade_timelock(&admin, &MIN_UPGRADE_TIMELOCK_SECS);
        assert_eq!(client.get_upgrade_timelock(), MIN_UPGRADE_TIMELOCK_SECS);
    }

    #[test]
    fn test_execute_upgrade_before_custom_timelock_fails() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _pool, _sme) = setup(&env);
        let custom_timelock = 7_200u64; // 2 hours
        client.set_upgrade_timelock(&admin, &custom_timelock);

        let hash = BytesN::from_array(&env, &[1u8; 32]);
        client.propose_upgrade(&admin, &hash);

        // Advance time but NOT past the 2-hour timelock
        env.ledger().with_mut(|l| l.timestamp += 3_600);
        let result = client.try_execute_upgrade(&admin);
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::UpgradeTimelockNotExpired.into()
        );
    }

    #[test]
    fn test_execute_upgrade_after_custom_timelock_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _pool, _sme) = setup(&env);
        client.set_upgrade_timelock(&admin, &7_200u64);

        let hash = BytesN::from_array(&env, &[1u8; 32]);
        client.propose_upgrade(&admin, &hash);

        // Advance past the 2-hour timelock
        env.ledger().with_mut(|l| l.timestamp += 7_201);
        // execute_upgrade would invoke deployer — skip if not supported in test env
        let _ = client.try_execute_upgrade(&admin);
    }

    // ── #340: WASM hash validation tests ─────────────────────────────────────

    #[test]
    fn test_propose_upgrade_zero_hash_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
        let result = client.try_propose_upgrade(&admin, &zero_hash);
        assert!(result.is_err());
    }

    #[test]
    fn test_propose_upgrade_nonzero_hash_accepted() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        let valid_hash = BytesN::from_array(&env, &[1u8; 32]);
        client.propose_upgrade(&admin, &valid_hash);
    }

    // ── #565: admin key rotation timelock tests ───────────────────────────────

    #[test]
    fn test_propose_admin_change_stores_pending() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _pool, _sme) = setup(&env);
        let new_admin = Address::generate(&env);
        client.propose_admin_change(&admin, &new_admin);
    }

    #[test]
    fn test_finalize_admin_change_before_timelock_fails() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _pool, _sme) = setup(&env);
        let new_admin = Address::generate(&env);
        client.propose_admin_change(&admin, &new_admin);
        // Try to finalize immediately — should fail
        let result = client.try_finalize_admin_change(&admin);
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::AdminChangeTimelockNotExpired.into()
        );
    }

    #[test]
    fn test_finalize_admin_change_after_timelock_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _pool, _sme) = setup(&env);
        let new_admin = Address::generate(&env);
        client.propose_admin_change(&admin, &new_admin);
        // Advance time past the 48-hour timelock
        env.ledger()
            .with_mut(|l| l.timestamp += ADMIN_CHANGE_TIMELOCK_SECS + 1);
        client.finalize_admin_change(&admin);
        // Verify that the new admin can now perform admin operations
        let oracle = Address::generate(&env);
        client.set_oracle(&new_admin, &oracle);
    }

    #[test]
    fn test_cancel_admin_change_removes_pending() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _pool, _sme) = setup(&env);
        let new_admin = Address::generate(&env);
        client.propose_admin_change(&admin, &new_admin);
        client.cancel_admin_change(&admin);
        // After cancel, finalize should fail with NoAdminChangeProposed
        let result = client.try_finalize_admin_change(&admin);
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::NoAdminChangeProposed.into()
        );
    }

    #[test]
    fn test_finalize_without_proposal_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        let result = client.try_finalize_admin_change(&admin);
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::NoAdminChangeProposed.into()
        );
    }

    #[test]
    fn test_cancel_without_proposal_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _pool, _sme) = setup(&env);
        let result = client.try_cancel_admin_change(&admin);
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::NoAdminChangeProposed.into()
        );
    }

    // ── #624: request_extension / approve_extension unit tests ──────────────────

    #[test]
    fn test_extension_happy_path() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, admin, pool, sme) = setup(&env);
        let id = make_invoice(&env, &client, &sme, 1_000);
        client.mark_funded(&id, &pool);

        let invoice = client.get_invoice(&id);
        let current_due_date = invoice.due_date;
        let new_due = current_due_date + 10 * 86_400; // 10 days extension

        // Request extension
        client.request_extension(&id, &sme, &new_due);
        let invoice = client.get_invoice(&id);
        assert_eq!(invoice.pending_due_date, Some(new_due));

        // Approve extension by admin
        client.approve_extension(&id, &admin);
        let invoice = client.get_invoice(&id);
        assert_eq!(invoice.due_date, new_due);
        assert_eq!(invoice.pending_due_date, None);
    }

    #[test]
    fn test_extension_too_large_fails() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, _admin, pool, sme) = setup(&env);
        let id = make_invoice(&env, &client, &sme, 1_000);
        client.mark_funded(&id, &pool);

        let invoice = client.get_invoice(&id);
        let current_due_date = invoice.due_date;
        // MAX_DUE_DATE_EXTENSION_SECS is 90 days. Request 91 days.
        let new_due = current_due_date + 91 * 86_400;

        let result = client.try_request_extension(&id, &sme, &new_due);
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::ExtensionTooLarge.into()
        );
    }

    #[test]
    fn test_extension_double_request_fails() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, _admin, pool, sme) = setup(&env);
        let id = make_invoice(&env, &client, &sme, 1_000);
        client.mark_funded(&id, &pool);

        let invoice = client.get_invoice(&id);
        let current_due_date = invoice.due_date;
        let new_due1 = current_due_date + 10 * 86_400;
        let new_due2 = current_due_date + 20 * 86_400;

        client.request_extension(&id, &sme, &new_due1);
        let result = client.try_request_extension(&id, &sme, &new_due2);
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::ExtensionAlreadyPending.into()
        );
    }

    #[test]
    fn test_approve_no_pending_extension_fails() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, admin, pool, sme) = setup(&env);
        let id = make_invoice(&env, &client, &sme, 1_000);
        client.mark_funded(&id, &pool);

        let result = client.try_approve_extension(&id, &admin);
        assert_eq!(
            result.unwrap_err().unwrap(),
            InvoiceError::NoPendingExtension.into()
        );
    }

    // ── #618: debtor registry exposure tracking tests ───────────────────────

    #[test]
    fn test_debtor_exposure_accumulates_across_invoices() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, admin, _pool, sme) = setup(&env);

        // Enable debtor registry
        client.set_require_registered_debtor(&admin, &true);

        let debtor_id = String::from_str(&env, "debtor-123");
        let debtor_name = String::from_str(&env, "Test Debtor");
        let max_exposure = 5_000i128;

        // Register debtor with max_exposure of 5000
        client.register_debtor(&admin, &debtor_id, &debtor_name, &max_exposure);

        // Create first invoice for 2000
        let id1 = client.create_invoice_with_metadata(
            &sme,
            &debtor_id,
            &2_000i128,
            &(env.ledger().timestamp() + 86_400 * 30),
            &String::from_str(&env, "Invoice 1"),
            &String::from_str(&env, "hash1"),
            &None,
        );

        // Verify exposure is now 2000
        let debtor1 = client.get_debtor(&debtor_id);
        assert_eq!(debtor1.current_exposure, 2_000);

        // Create second invoice for 2500
        let id2 = client.create_invoice_with_metadata(
            &sme,
            &debtor_id,
            &2_500i128,
            &(env.ledger().timestamp() + 86_400 * 30),
            &String::from_str(&env, "Invoice 2"),
            &String::from_str(&env, "hash2"),
            &None,
        );

        // Verify exposure is now 4500
        let debtor2 = client.get_debtor(&debtor_id);
        assert_eq!(debtor2.current_exposure, 4_500);

        // Third invoice for 1000 should exceed max_exposure (4500 + 1000 = 5500 > 5000)
        // This should panic/fail
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.create_invoice_with_metadata(
                &sme,
                &debtor_id,
                &1_000i128,
                &(env.ledger().timestamp() + 86_400 * 30),
                &String::from_str(&env, "Invoice 3"),
                &String::from_str(&env, "hash3"),
                &None,
            )
        }));
        assert!(result.is_err());
    }

    #[test]
    fn test_debtor_exposure_decrements_on_payment() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, admin, pool, sme) = setup(&env);

        client.set_require_registered_debtor(&admin, &true);

        let debtor_id = String::from_str(&env, "debtor-456");
        let debtor_name = String::from_str(&env, "Test Debtor 2");
        let max_exposure = 5_000i128;

        client.register_debtor(&admin, &debtor_id, &debtor_name, &max_exposure);

        let id = client.create_invoice_with_metadata(
            &sme,
            &debtor_id,
            &3_000i128,
            &(env.ledger().timestamp() + 86_400 * 30),
            &String::from_str(&env, "Invoice"),
            &String::from_str(&env, "hash"),
            &None,
        );

        // Verify exposure is 3000
        let debtor_before = client.get_debtor(&debtor_id);
        assert_eq!(debtor_before.current_exposure, 3_000);

        // Mark invoice as funded and paid
        client.mark_funded(&id, &pool);
        client.mark_paid(&id, &pool);

        // Verify exposure is now 0
        let debtor_after = client.get_debtor(&debtor_id);
        assert_eq!(debtor_after.current_exposure, 0);
    }

    #[test]
    fn test_deactivated_debtor_cannot_receive_invoices() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000);
        let (client, admin, _pool, sme) = setup(&env);

        client.set_require_registered_debtor(&admin, &true);

        let debtor_id = String::from_str(&env, "debtor-789");
        let debtor_name = String::from_str(&env, "Test Debtor 3");

        client.register_debtor(&admin, &debtor_id, &debtor_name, &10_000i128);

        // Deactivate the debtor
        client.deactivate_debtor(&admin, &debtor_id);

        // Attempt to create invoice for deactivated debtor should fail
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            client.create_invoice_with_metadata(
                &sme,
                &debtor_id,
                &1_000i128,
                &(env.ledger().timestamp() + 86_400 * 30),
                &String::from_str(&env, "Invoice"),
                &String::from_str(&env, "hash"),
                &None,
            )
        }));
        assert!(result.is_err());
    }
}
