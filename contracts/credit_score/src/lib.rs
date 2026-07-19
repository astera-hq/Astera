#![no_std]

// === AUTHORIZED CALLERS ===
// - Admin: initialize(), admin-only setters
// - Pool contract: record_payment(), record_default() (pool address stored in config)
// - Anyone: view functions (get_credit_score)

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    BytesN, Env, String, Symbol, Vec,
};

/// #396: Typed error codes for the credit-score contract.
/// All error codes are stable — do not re-number existing entries.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CreditScoreError {
    /// Contract has already been initialised.
    AlreadyInitialized = 1,
    /// Caller is not the contract admin.
    Unauthorized = 2,
    /// Contract is paused; state-changing calls are blocked.
    ContractPaused = 3,
    /// This invoice has already been recorded in the credit score.
    InvoiceAlreadyProcessed = 4,
    /// Score thresholds are not strictly decreasing.
    InvalidThresholds = 5,
    /// Late-payment threshold is outside the valid 1–365 day range.
    InvalidLateThreshold = 6,
    /// Payment history limit must be greater than zero.
    PaymentHistoryLimitZero = 7,
    /// Upgrade timelock has not yet elapsed.
    UpgradeTimelockNotExpired = 8,
    /// No upgrade has been proposed.
    NoUpgradeProposed = 9,
    /// #338: upgrade timelock value is below the allowed minimum.
    InvalidUpgradeTimelock = 10,
    /// #340: proposed WASM hash is all-zero (invalid).
    InvalidWasmHash = 11,
    /// #565: admin change already pending.
    AdminChangePending = 12,
    /// #565: admin change timelock has not elapsed.
    AdminChangeTimelockNotExpired = 13,
    /// #565: no admin change has been proposed.
    NoAdminChangeProposed = 14,
}

/// Semantic version of this credit-score contract (#237).
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CreditScoreVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

fn parse_credit_score_version() -> CreditScoreVersion {
    let v = env!("CARGO_PKG_VERSION");
    let mut parts = v.splitn(3, '.');
    let major = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let minor = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|s| s.split('-').next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    CreditScoreVersion {
        major,
        minor,
        patch,
    }
}

pub const MIN_SCORE: u32 = 200;
pub const MAX_SCORE: u32 = 850;
pub const BASE_SCORE: u32 = 500;

const PTS_PAID_ON_TIME: u32 = 30;
const PTS_PAID_LATE: u32 = 15;
const PTS_DEFAULTED: i32 = -50;
const PTS_AVG_PAYMENT_NEGATIVE: i32 = 20;
const PTS_AVG_PAYMENT_LT3: i32 = 15;
const PTS_AVG_PAYMENT_LT7: i32 = 10;
const PTS_AVG_PAYMENT_OVER_LATE: i32 = -15;
const INVOICE_BONUS_THRESHOLD_1: u32 = 5;
const INVOICE_BONUS_THRESHOLD_2: u32 = 10;
const INVOICE_BONUS_THRESHOLD_3: u32 = 20;
const INVOICE_BONUS_POINTS: i32 = 5;
const VOLUME_BONUS_THRESHOLD_1: i128 = 1_000_000_000;
const VOLUME_BONUS_THRESHOLD_2: i128 = 10_000_000_000;
const VOLUME_BONUS_THRESHOLD_3: i128 = 100_000_000_000;
const VOLUME_BONUS_POINTS_1: i32 = 5;
const VOLUME_BONUS_POINTS_2: i32 = 15;
const VOLUME_BONUS_POINTS_3: i32 = 25;
/// #568: minimum payment amount for an invoice to count toward milestone
/// bonuses. Default = $100 USDC at 6 decimals. Blocks micro-invoice cycling
/// (1-stroop invoices) from farming the +5pt milestone awards at 5/10/20.
const DEFAULT_MIN_MILESTONE_VOLUME: i128 = 100_000_000;

const LATE_PAYMENT_THRESHOLD_SECS: u64 = 7 * 24 * 60 * 60;
const UPGRADE_TIMELOCK_SECS: u64 = 86400; // 24 hours — default
const MIN_UPGRADE_TIMELOCK_SECS: u64 = 3_600; // 1 hour minimum (#338)
const ADMIN_CHANGE_TIMELOCK_SECS: u64 = 172_800; // 48 hours — #565
/// Current on-chain storage schema version (#397). Bump by one and add a
/// matching arm in `run_migration` whenever the persistent storage layout
/// changes so a deployed contract can migrate state after a WASM upgrade.
const CURRENT_MIGRATION_VERSION: u32 = 1;
pub const MAX_PAYMENT_HISTORY: u32 = 100;

/// Max page size for list_all_sme_stats to prevent gas exhaustion.
const MAX_SME_PAGE_SIZE: u32 = 100;

#[contracttype]
#[derive(Clone)]
pub struct PaymentRecord {
    pub invoice_id: u64,
    pub sme: Address,
    pub amount: i128,
    pub due_date: u64,
    pub paid_at: u64,
    pub status: PaymentStatus,
    pub days_late: i64,
}

#[contracttype]
#[derive(Clone, PartialEq, Debug)]
pub enum PaymentStatus {
    PaidOnTime,
    PaidLate,
    Defaulted,
}

#[contracttype]
#[derive(Clone)]
pub struct CreditScoreData {
    pub sme: Address,
    pub score: u32,
    pub total_invoices: u32,
    pub paid_on_time: u32,
    pub paid_late: u32,
    pub defaulted: u32,
    pub total_volume: i128,
    pub average_payment_days: i64,
    pub last_updated: u64,
    pub score_version: u32,
}

#[contracttype]
#[derive(Clone)]
pub struct SmeScoreEntry {
    pub sme: Address,
    pub score: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ScoreCoreConfig {
    pub paid_on_time_pts: i32,
    pub paid_late_pts: i32,
    pub defaulted_pts: i32,
    pub base_score: u32,
    pub min_score: u32,
    pub max_score: u32,
    pub score_version: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ScoreAverageConfig {
    pub avg_neg_pts: i32,
    pub avg_lt3_pts: i32,
    pub avg_lt7_pts: i32,
    pub avg_over_late_pts: i32,
    pub avg_days_lt3: i64,
    pub avg_days_lt7: i64,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ScoreBonusConfig {
    pub inv_bonus_thr1: u32,
    pub inv_bonus_thr2: u32,
    pub inv_bonus_thr3: u32,
    pub inv_bonus_pts: i32,
    pub vol_bonus_thr1: i128,
    pub vol_bonus_thr2: i128,
    pub vol_bonus_thr3: i128,
    pub vol_bonus_pts1: i32,
    pub vol_bonus_pts2: i32,
    pub vol_bonus_pts3: i32,
    /// #568: Minimum invoice amount that contributes to milestone counters.
    /// Invoices below this threshold still update total counts/volume but do
    /// not increment the milestone counter used for inv_bonus_thr1/2/3.
    pub min_milestone_volume: i128,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ScoringConfig {
    pub core: ScoreCoreConfig,
    pub averages: ScoreAverageConfig,
    pub bonuses: ScoreBonusConfig,
}

impl ScoringConfig {
    pub fn defaults() -> Self {
        Self {
            core: ScoreCoreConfig {
                paid_on_time_pts: PTS_PAID_ON_TIME as i32,
                paid_late_pts: PTS_PAID_LATE as i32,
                defaulted_pts: PTS_DEFAULTED,
                base_score: BASE_SCORE,
                min_score: MIN_SCORE,
                max_score: MAX_SCORE,
                score_version: 1,
            },
            averages: ScoreAverageConfig {
                avg_neg_pts: PTS_AVG_PAYMENT_NEGATIVE,
                avg_lt3_pts: PTS_AVG_PAYMENT_LT3,
                avg_lt7_pts: PTS_AVG_PAYMENT_LT7,
                avg_over_late_pts: PTS_AVG_PAYMENT_OVER_LATE,
                avg_days_lt3: 3,
                avg_days_lt7: 7,
            },
            bonuses: ScoreBonusConfig {
                inv_bonus_thr1: INVOICE_BONUS_THRESHOLD_1,
                inv_bonus_thr2: INVOICE_BONUS_THRESHOLD_2,
                inv_bonus_thr3: INVOICE_BONUS_THRESHOLD_3,
                inv_bonus_pts: INVOICE_BONUS_POINTS,
                vol_bonus_thr1: VOLUME_BONUS_THRESHOLD_1,
                vol_bonus_thr2: VOLUME_BONUS_THRESHOLD_2,
                vol_bonus_thr3: VOLUME_BONUS_THRESHOLD_3,
                vol_bonus_pts1: VOLUME_BONUS_POINTS_1,
                vol_bonus_pts2: VOLUME_BONUS_POINTS_2,
                vol_bonus_pts3: VOLUME_BONUS_POINTS_3,
                min_milestone_volume: DEFAULT_MIN_MILESTONE_VOLUME,
            },
        }
    }
}

#[contracttype]
#[derive(Clone)]
pub struct ScoreThresholds {
    pub excellent: u32,
    pub very_good: u32,
    pub good: u32,
    pub fair: u32,
}

impl ScoreThresholds {
    pub fn defaults() -> Self {
        Self {
            excellent: 800,
            very_good: 740,
            good: 670,
            fair: 580,
        }
    }
}

/// Returned by `get_credit_score`. Includes the current config version alongside the
/// stored score so callers can detect staleness in a single call without a separate
/// `get_scoring_config()` round-trip.
///
/// `is_stale` is true when `score_version` (the config version active when the score
/// was last computed) does not match `config_version` (the config version now active).
/// A stale flag means the stored score was computed under different scoring parameters
/// and should be treated as approximate until the SME's next payment is recorded.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CreditScoreResponse {
    pub sme: Address,
    pub score: u32,
    pub total_invoices: u32,
    pub paid_on_time: u32,
    pub paid_late: u32,
    pub defaulted: u32,
    pub total_volume: i128,
    pub average_payment_days: i64,
    pub last_updated: u64,
    /// Config version that was active when this score was last computed.
    pub score_version: u32,
    /// Config version currently active on the contract.
    pub config_version: u32,
    /// True when `score_version != config_version` — the score is stale.
    pub is_stale: bool,
}

#[contracttype]
pub enum DataKey {
    CreditScore(Address),
    /// Number of retained records in the rolling payment history window.
    PaymentHistory(Address),
    PaymentHistoryStart(Address),
    PaymentRecordIdx(Address, u32),
    PaymentRecordScoreVersion(u64),
    InvoiceProcessed(u64),
    ScoringConfig,
    Admin,
    InvoiceContract,
    PoolContract,
    Initialized,
    ScoreVersion,
    /// Size of the rolling payment-history window retained per SME.
    MaxPaymentHistory,
    Paused,
    ProposedWasmHash,
    UpgradeScheduledAt,
    /// Semantic version stored during initialize() (#237).
    ContractVersion,
    /// Applied storage-schema migration level (#397).
    MigrationVersion,
    /// Configurable late-payment threshold in days (#430).
    LateThreshold,
    /// #428: Configurable score thresholds (Excellent, Very Good, Good, Fair)
    ScoreThresholds,
    /// #338: configurable upgrade timelock duration in seconds
    UpgradeTimelockSecs,
    /// #568: Count of invoices per SME with amount >= min_milestone_volume,
    /// excluding defaults. Used as the input to milestone bonus thresholds so
    /// that micro-invoice cycling cannot inflate the score.
    MilestoneCount(Address),
    /// #565: admin key rotation timelock
    PendingAdmin,
    AdminChangeScheduledAt,
    SmeByIndex(u32),
    SmeCount,
}

const EVT: Symbol = symbol_short!("CREDIT");

fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        panic_with_error!(env, CreditScoreError::ContractPaused);
    }
}

#[contract]
pub struct CreditScoreContract;

fn get_late_threshold(env: &Env) -> i64 {
    env.storage()
        .persistent()
        .get(&DataKey::LateThreshold)
        .unwrap_or(30)
}

fn max_payment_history(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::MaxPaymentHistory)
        .unwrap_or(MAX_PAYMENT_HISTORY)
}

fn load_scoring_config(env: &Env) -> ScoringConfig {
    env.storage()
        .persistent()
        .get(&DataKey::ScoringConfig)
        .unwrap_or_else(ScoringConfig::defaults)
}

fn calculate_days_late(due_date: u64, paid_at: u64) -> i64 {
    if paid_at > due_date {
        ((paid_at - due_date - 1) as i64 / (24 * 60 * 60)) + 1
    } else {
        -((due_date - paid_at) as i64 / (24 * 60 * 60))
    }
}

#[allow(clippy::too_many_arguments)]
fn calculate_score_with_config(
    env: &Env,
    config: &ScoringConfig,
    late_threshold: i64,
    total_invoices: u32,
    paid_on_time: u32,
    paid_late: u32,
    defaulted: u32,
    total_volume: i128,
    average_payment_days: i64,
    // #568: number of paid invoices >= min_milestone_volume. Drives the
    // 5/10/20-invoice milestone bonuses so micro-invoices don't qualify.
    milestone_count: u32,
) -> u32 {
    if total_invoices == 0 {
        return MIN_SCORE;
    }

    // Validate internal consistency: paid_on_time + paid_late + defaulted must equal
    // total_invoices. A mismatch signals corrupted storage; we emit a warning event and
    // proceed with best-effort scoring rather than panicking in production.
    let counted = paid_on_time
        .saturating_add(paid_late)
        .saturating_add(defaulted);
    if counted != total_invoices {
        env.events().publish(
            (
                Symbol::new(env, "CREDIT"),
                Symbol::new(env, "data_inconsistency"),
            ),
            counted,
        );
    }

    // total_volume must be non-negative; negative values can produce incorrect score boosts.
    if total_volume < 0 {
        env.events().publish(
            (
                Symbol::new(env, "CREDIT"),
                Symbol::new(env, "data_inconsistency"),
            ),
            total_volume,
        );
    }

    let mut score: i64 = config.core.base_score as i64;

    score += (paid_on_time as i32 * config.core.paid_on_time_pts) as i64;
    score += (paid_late as i32 * config.core.paid_late_pts) as i64;
    score += (defaulted as i32 * config.core.defaulted_pts) as i64;

    // #568: gate milestone bonuses on the milestone_count (only invoices >=
    // min_milestone_volume) so that micro-invoice cycling cannot farm them.
    if milestone_count >= config.bonuses.inv_bonus_thr1 {
        score += config.bonuses.inv_bonus_pts as i64;
    }
    if milestone_count >= config.bonuses.inv_bonus_thr2 {
        score += config.bonuses.inv_bonus_pts as i64;
    }
    if milestone_count >= config.bonuses.inv_bonus_thr3 {
        score += config.bonuses.inv_bonus_pts as i64;
    }

    if average_payment_days < 0 {
        score += config.averages.avg_neg_pts as i64;
    } else if average_payment_days < config.averages.avg_days_lt3 {
        score += config.averages.avg_lt3_pts as i64;
    } else if average_payment_days < config.averages.avg_days_lt7 {
        score += config.averages.avg_lt7_pts as i64;
    } else if average_payment_days > late_threshold {
        score += config.averages.avg_over_late_pts as i64;
    }

    if total_volume > config.bonuses.vol_bonus_thr3 {
        score += config.bonuses.vol_bonus_pts3 as i64;
    } else if total_volume > config.bonuses.vol_bonus_thr2 {
        score += config.bonuses.vol_bonus_pts2 as i64;
    } else if total_volume > config.bonuses.vol_bonus_thr1 {
        score += config.bonuses.vol_bonus_pts1 as i64;
    }

    if score < config.core.min_score as i64 {
        config.core.min_score
    } else if score > config.core.max_score as i64 {
        config.core.max_score
    } else {
        score as u32
    }
}

#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn calculate_score(
    env: &Env,
    late_threshold: i64,
    total_invoices: u32,
    paid_on_time: u32,
    paid_late: u32,
    defaulted: u32,
    total_volume: i128,
    average_payment_days: i64,
) -> u32 {
    let config = load_scoring_config(env);
    // Test helper: assume every non-default invoice was milestone-eligible
    // so historical fuzz cases keep their original semantics.
    let milestone_count = paid_on_time + paid_late;
    calculate_score_with_config(
        env,
        &config,
        late_threshold,
        total_invoices,
        paid_on_time,
        paid_late,
        defaulted,
        total_volume,
        average_payment_days,
        milestone_count,
    )
}

fn calculate_average_payment_days(paid_on_time: u32, paid_late: u32, total_late_days: i64) -> i64 {
    let total_paid = paid_on_time + paid_late;
    if total_paid == 0 {
        return 0;
    }
    total_late_days / total_paid as i64
}

#[contractimpl]
impl CreditScoreContract {
    pub fn initialize(env: Env, admin: Address, invoice_contract: Address, pool_contract: Address) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic_with_error!(&env, CreditScoreError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&DataKey::InvoiceContract, &invoice_contract);
        env.storage()
            .instance()
            .set(&DataKey::PoolContract, &pool_contract);
        env.storage().instance().set(&DataKey::ScoreVersion, &1u32);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage()
            .instance()
            .set(&DataKey::MaxPaymentHistory, &MAX_PAYMENT_HISTORY);
        env.storage()
            .persistent()
            .set(&DataKey::ScoringConfig, &ScoringConfig::defaults());
        // Store compile-time version (#237)
        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &parse_credit_score_version());
        env.storage()
            .instance()
            .set(&DataKey::MigrationVersion, &0u32);
    }

    /// Returns the semantic version of this deployed credit-score contract (#237).
    pub fn version(env: Env) -> CreditScoreVersion {
        env.storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap_or_else(parse_credit_score_version)
    }

    /// Returns the applied storage-schema migration level (#397).
    pub fn migration_version(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::MigrationVersion)
            .unwrap_or(0)
    }

    /// Run pending storage migrations after a WASM upgrade (#397).
    ///
    /// Admin-only and idempotent: once the contract has reached
    /// `CURRENT_MIGRATION_VERSION` further calls are a no-op. Each migration
    /// step transforms the persistent storage layout for one schema version
    /// and is meant to be invoked manually after `execute_upgrade`.
    pub fn run_migration(env: Env, admin: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        let current: u32 = env
            .storage()
            .instance()
            .get(&DataKey::MigrationVersion)
            .unwrap_or(0);
        if current >= CURRENT_MIGRATION_VERSION {
            return;
        }
        // Future migration arms (current -> current + 1) transform storage here.
        env.storage()
            .instance()
            .set(&DataKey::MigrationVersion, &CURRENT_MIGRATION_VERSION);
    }

    pub fn pause(env: Env, admin: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events().publish((EVT, symbol_short!("paused")), admin);
    }

    pub fn unpause(env: Env, admin: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events()
            .publish((EVT, symbol_short!("unpaused")), admin);
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<DataKey, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Shared helper (#404): appends a payment record, updates counters,
    /// recalculates score, persists everything, and marks the invoice processed.
    /// Returns the updated credit data so callers can emit their event payload.
    fn commit_payment_record(
        env: &Env,
        sme: &Address,
        invoice_id: u64,
        record: PaymentRecord,
    ) -> CreditScoreData {
        let scoring_config = load_scoring_config(env);
        let mut credit_data = Self::get_or_create_credit_data(env, sme);

        let max_history = max_payment_history(env);
        let history_len: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentHistory(sme.clone()))
            .unwrap_or(0);
        let start_idx: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentHistoryStart(sme.clone()))
            .unwrap_or(0);

        if history_len < max_history {
            env.storage().persistent().set(
                &DataKey::PaymentRecordIdx(sme.clone(), history_len),
                &record,
            );
            env.storage()
                .instance()
                .set(&DataKey::PaymentHistory(sme.clone()), &(history_len + 1));
        } else {
            env.storage()
                .persistent()
                .set(&DataKey::PaymentRecordIdx(sme.clone(), start_idx), &record);
            let new_start = (start_idx + 1) % max_history;
            env.storage()
                .instance()
                .set(&DataKey::PaymentHistoryStart(sme.clone()), &new_start);
        }
        env.storage().persistent().set(
            &DataKey::PaymentRecordScoreVersion(invoice_id),
            &scoring_config.core.score_version,
        );

        // Capture previous paid count before incrementing, for the running average.
        let prev_paid = (credit_data.paid_on_time + credit_data.paid_late) as i64;

        match record.status {
            PaymentStatus::PaidOnTime => {
                credit_data.paid_on_time += 1;
            }
            PaymentStatus::PaidLate => {
                credit_data.paid_late += 1;
            }
            PaymentStatus::Defaulted => {
                credit_data.defaulted += 1;
            }
        }

        credit_data.total_invoices += 1;
        credit_data.total_volume += record.amount;

        // #568: only successful payments above the minimum milestone volume
        // contribute to milestone counts. Defaults are excluded because they
        // reflect failure, not economic activity.
        let mut milestone_count: u32 = env
            .storage()
            .persistent()
            .get(&DataKey::MilestoneCount(sme.clone()))
            .unwrap_or(0);
        if record.status != PaymentStatus::Defaulted
            && record.amount >= scoring_config.bonuses.min_milestone_volume
        {
            milestone_count += 1;
            env.storage()
                .persistent()
                .set(&DataKey::MilestoneCount(sme.clone()), &milestone_count);
        }
        // Only paid (on-time + late) invoices contribute to the average; defaults are excluded.
        // Running sum = previous_average * previous_paid_count + new_days_late
        let days_late = record.days_late;
        if record.status != PaymentStatus::Defaulted {
            credit_data.average_payment_days = calculate_average_payment_days(
                credit_data.paid_on_time,
                credit_data.paid_late,
                credit_data.average_payment_days * prev_paid + days_late,
            );
        }
        credit_data.score = calculate_score_with_config(
            env,
            &scoring_config,
            get_late_threshold(env),
            credit_data.total_invoices,
            credit_data.paid_on_time,
            credit_data.paid_late,
            credit_data.defaulted,
            credit_data.total_volume,
            credit_data.average_payment_days,
            milestone_count,
        );
        credit_data.score_version = scoring_config.core.score_version;
        credit_data.last_updated = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::CreditScore(sme.clone()), &credit_data);
        env.storage()
            .persistent()
            .set(&DataKey::InvoiceProcessed(invoice_id), &true);

        credit_data
    }

    #[allow(clippy::too_many_arguments)]
    fn record_invoice_outcome(
        env: &Env,
        invoice_id: u64,
        sme: &Address,
        amount: i128,
        due_date: u64,
        paid_at: u64,
        status: PaymentStatus,
        days_late: i64,
    ) -> CreditScoreData {
        let record = PaymentRecord {
            invoice_id,
            sme: sme.clone(),
            amount,
            due_date,
            paid_at,
            status,
            days_late,
        };

        Self::commit_payment_record(env, sme, invoice_id, record)
    }

    pub fn record_payment(
        env: Env,
        _caller: Address,
        invoice_id: u64,
        sme: Address,
        amount: i128,
        due_date: u64,
        paid_at: u64,
    ) {
        let pool: Address = env
            .storage()
            .instance()
            .get(&DataKey::PoolContract)
            .expect("not initialized");

        pool.require_auth();

        require_not_paused(&env);

        if env
            .storage()
            .persistent()
            .has(&DataKey::InvoiceProcessed(invoice_id))
        {
            panic_with_error!(&env, CreditScoreError::InvoiceAlreadyProcessed);
        }

        let status = if paid_at <= due_date {
            PaymentStatus::PaidOnTime
        } else if paid_at <= due_date + LATE_PAYMENT_THRESHOLD_SECS {
            PaymentStatus::PaidLate
        } else {
            PaymentStatus::Defaulted
        };

        let days_late = calculate_days_late(due_date, paid_at);
        let credit_data = Self::record_invoice_outcome(
            &env,
            invoice_id,
            &sme,
            amount,
            due_date,
            paid_at,
            status.clone(),
            days_late,
        );

        env.events().publish(
            (EVT, symbol_short!("payment")),
            (
                sme,
                invoice_id,
                status,
                credit_data.score,
                env.ledger().timestamp(),
            ),
        );
    }

    /// Record that the borrower's invoice has been successfully funded (#534).
    /// Being funded is a positive demand signal — lenders deployed capital,
    /// which is evidence of creditworthiness. The weight is intentionally light
    /// so funding events alone cannot override a poor repayment history.
    ///
    /// Idempotent: duplicate calls for the same invoice_id are silently ignored.
    pub fn record_funding(env: Env, _caller: Address, invoice_id: u64, sme: Address, amount: i128) {
        let pool: Address = env
            .storage()
            .instance()
            .get(&DataKey::PoolContract)
            .expect("not initialized");

        pool.require_auth();

        require_not_paused(&env);

        if env
            .storage()
            .persistent()
            .has(&DataKey::InvoiceProcessed(invoice_id))
        {
            return;
        }

        let mut credit_data = Self::get_or_create_credit_data(&env, &sme);
        credit_data.total_volume = credit_data.total_volume.saturating_add(amount);
        credit_data.last_updated = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::CreditScore(sme.clone()), &credit_data);

        env.events().publish(
            (EVT, symbol_short!("funded")),
            (sme, invoice_id, amount, env.ledger().timestamp()),
        );
    }

    pub fn record_default(
        env: Env,
        _caller: Address,
        invoice_id: u64,
        sme: Address,
        amount: i128,
        due_date: u64,
    ) {
        let pool: Address = env
            .storage()
            .instance()
            .get(&DataKey::PoolContract)
            .expect("not initialized");

        pool.require_auth();

        require_not_paused(&env);

        if env
            .storage()
            .persistent()
            .has(&DataKey::InvoiceProcessed(invoice_id))
        {
            panic_with_error!(&env, CreditScoreError::InvoiceAlreadyProcessed);
        }

        let defaulted_at = env.ledger().timestamp();
        let days_late = calculate_days_late(due_date, defaulted_at);
        let credit_data = Self::record_invoice_outcome(
            &env,
            invoice_id,
            &sme,
            amount,
            due_date,
            defaulted_at,
            PaymentStatus::Defaulted,
            days_late,
        );

        env.events().publish(
            (EVT, symbol_short!("default")),
            (sme, invoice_id, credit_data.score, env.ledger().timestamp()),
        );
    }

    pub fn get_credit_score(env: Env, sme: Address) -> CreditScoreResponse {
        let data = Self::get_or_create_credit_data(&env, &sme);
        let config_version = load_scoring_config(&env).core.score_version;
        CreditScoreResponse {
            sme: data.sme,
            score: data.score,
            total_invoices: data.total_invoices,
            paid_on_time: data.paid_on_time,
            paid_late: data.paid_late,
            defaulted: data.defaulted,
            total_volume: data.total_volume,
            average_payment_days: data.average_payment_days,
            last_updated: data.last_updated,
            score_version: data.score_version,
            config_version,
            is_stale: data.score_version != config_version,
        }
    }

    pub fn list_all_sme_stats(env: Env, page: u32, page_size: u32) -> Vec<SmeScoreEntry> {
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::SmeCount)
            .unwrap_or(0);
        let size = if page_size > MAX_SME_PAGE_SIZE {
            MAX_SME_PAGE_SIZE
        } else {
            page_size
        };
        let start = page * size;
        if start >= count {
            return Vec::new(&env);
        }
        let end = (start + size).min(count);
        let mut entries = Vec::new(&env);
        let mut i = start;
        while i < end {
            if let Some(sme) = env
                .storage()
                .persistent()
                .get::<DataKey, Address>(&DataKey::SmeByIndex(i))
            {
                let data = Self::get_or_create_credit_data(&env, &sme);
                entries.push_back(SmeScoreEntry {
                    sme,
                    score: data.score,
                });
            }
            i += 1;
        }
        entries
    }

    pub fn get_payment_history(env: Env, sme: Address) -> Vec<PaymentRecord> {
        let history_len: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentHistory(sme.clone()))
            .unwrap_or(0);
        let start_idx: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentHistoryStart(sme.clone()))
            .unwrap_or(0);
        let max_history = max_payment_history(&env);

        let mut records = Vec::new(&env);
        for offset in 0..history_len {
            let i = (start_idx + offset) % max_history;
            if let Some(record) = env
                .storage()
                .persistent()
                .get(&DataKey::PaymentRecordIdx(sme.clone(), i))
            {
                records.push_back(record);
            }
        }
        records
    }

    pub fn get_payment_record_score_version(env: Env, invoice_id: u64) -> Option<u32> {
        env.storage()
            .persistent()
            .get(&DataKey::PaymentRecordScoreVersion(invoice_id))
    }

    pub fn get_payment_record(env: Env, sme: Address, index: u32) -> Option<PaymentRecord> {
        let history_len: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentHistory(sme.clone()))
            .unwrap_or(0);
        if index >= history_len {
            return None;
        }
        let start_idx: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentHistoryStart(sme.clone()))
            .unwrap_or(0);
        let max_history = max_payment_history(&env);
        let idx = (start_idx + index) % max_history;
        env.storage()
            .persistent()
            .get(&DataKey::PaymentRecordIdx(sme, idx))
    }

    pub fn get_payment_history_length(env: Env, sme: Address) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::PaymentHistory(sme))
            .unwrap_or(0)
    }

    pub fn get_score_band(env: Env, score: u32) -> String {
        let thresholds = Self::get_score_thresholds(&env);
        let config = load_scoring_config(&env);
        if score >= thresholds.excellent {
            String::from_str(&env, "Excellent")
        } else if score >= thresholds.very_good {
            String::from_str(&env, "Very Good")
        } else if score >= thresholds.good {
            String::from_str(&env, "Good")
        } else if score >= thresholds.fair {
            String::from_str(&env, "Fair")
        } else if score >= config.core.base_score {
            String::from_str(&env, "Poor")
        } else {
            String::from_str(&env, "Very Poor")
        }
    }

    pub fn get_score_thresholds(env: &Env) -> ScoreThresholds {
        env.storage()
            .persistent()
            .get(&DataKey::ScoreThresholds)
            .unwrap_or_else(ScoreThresholds::defaults)
    }

    pub fn set_score_thresholds(env: Env, admin: Address, thresholds: ScoreThresholds) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        require_not_paused(&env);
        // Validate thresholds are strictly decreasing
        if !(thresholds.excellent > thresholds.very_good
            && thresholds.very_good > thresholds.good
            && thresholds.good > thresholds.fair
            && thresholds.fair > BASE_SCORE)
        {
            panic_with_error!(&env, CreditScoreError::InvalidThresholds);
        }
        let old = Self::get_score_thresholds(&env);
        env.storage()
            .persistent()
            .set(&DataKey::ScoreThresholds, &thresholds);
        env.events().publish(
            (EVT, symbol_short!("thresh")),
            (old.excellent, old.very_good, old.good, old.fair),
        );
    }

    pub fn get_scoring_config(env: Env) -> ScoringConfig {
        load_scoring_config(&env)
    }

    pub fn set_scoring_config(env: Env, admin: Address, config: ScoringConfig) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        require_not_paused(&env);
        if !(config.core.min_score <= config.core.base_score
            && config.core.base_score <= config.core.max_score)
        {
            panic!("invalid scoring config: min/base/max scores out of order");
        }
        if config.averages.avg_days_lt3 >= config.averages.avg_days_lt7 {
            panic!("invalid scoring config: average-payment thresholds must increase");
        }
        if !(config.bonuses.inv_bonus_thr1 < config.bonuses.inv_bonus_thr2
            && config.bonuses.inv_bonus_thr2 < config.bonuses.inv_bonus_thr3
            && config.bonuses.vol_bonus_thr1 < config.bonuses.vol_bonus_thr2
            && config.bonuses.vol_bonus_thr2 < config.bonuses.vol_bonus_thr3)
        {
            panic!("invalid scoring config: thresholds must increase");
        }
        if config.core.score_version == 0 {
            panic!("invalid scoring config: score_version must be positive");
        }
        if config.core.paid_on_time_pts <= 0
            || config.core.paid_late_pts <= 0
            || config.bonuses.inv_bonus_pts <= 0
            || config.bonuses.vol_bonus_pts1 <= 0
            || config.bonuses.vol_bonus_pts2 <= 0
            || config.bonuses.vol_bonus_pts3 <= 0
        {
            panic!("invalid scoring config: positive point values required");
        }

        let old = load_scoring_config(&env);
        if config.core.score_version <= old.core.score_version {
            panic!("invalid scoring config: score_version must increase");
        }

        env.storage()
            .persistent()
            .set(&DataKey::ScoringConfig, &config);
        env.storage()
            .instance()
            .set(&DataKey::ScoreVersion, &config.core.score_version);
        env.events().publish(
            (EVT, symbol_short!("score_cfg")),
            (old.core.score_version, config.core.score_version),
        );
    }

    pub fn is_invoice_processed(env: Env, invoice_id: u64) -> bool {
        env.storage()
            .persistent()
            .has(&DataKey::InvoiceProcessed(invoice_id))
    }

    pub fn get_config(env: Env) -> (Address, Address, Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        let invoice_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::InvoiceContract)
            .expect("not initialized");
        let pool_contract: Address = env
            .storage()
            .instance()
            .get(&DataKey::PoolContract)
            .expect("not initialized");
        (admin, invoice_contract, pool_contract)
    }

    pub fn set_invoice_contract(env: Env, admin: Address, invoice_contract: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        require_not_paused(&env);
        env.storage()
            .instance()
            .set(&DataKey::InvoiceContract, &invoice_contract);
        env.events()
            .publish((EVT, symbol_short!("set_inv")), (admin, invoice_contract));
    }

    pub fn set_pool_contract(env: Env, admin: Address, pool_contract: Address) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        require_not_paused(&env);
        env.storage()
            .instance()
            .set(&DataKey::PoolContract, &pool_contract);
        env.events()
            .publish((EVT, symbol_short!("set_pc")), (admin, pool_contract));
    }

    /// Set the late-payment threshold (in days) used in score calculation.
    /// Default is 30 days. Valid range: 1–365.
    pub fn set_late_threshold(env: Env, admin: Address, days: i64) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        if !(1..=365).contains(&days) {
            panic_with_error!(&env, CreditScoreError::InvalidLateThreshold);
        }
        env.storage()
            .persistent()
            .set(&DataKey::LateThreshold, &days);
        env.events().publish((EVT, symbol_short!("lt_upd")), days);
    }

    /// Returns the current late-payment threshold in days (default 30).
    pub fn get_late_threshold(env: Env) -> i64 {
        env.storage()
            .persistent()
            .get(&DataKey::LateThreshold)
            .unwrap_or(30)
    }

    pub fn set_max_payment_history(env: Env, admin: Address, max_history: u32) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        require_not_paused(&env);
        if max_history == 0 {
            panic_with_error!(&env, CreditScoreError::PaymentHistoryLimitZero);
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxPaymentHistory, &max_history);
        env.events()
            .publish((EVT, symbol_short!("hist_upd")), max_history);
    }

    pub fn get_max_payment_history(env: Env) -> u32 {
        max_payment_history(&env)
    }

    fn get_or_create_credit_data(env: &Env, sme: &Address) -> CreditScoreData {
        if let Some(data) = env
            .storage()
            .persistent()
            .get(&DataKey::CreditScore(sme.clone()))
        {
            data
        } else {
            let count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::SmeCount)
                .unwrap_or(0);
            env.storage()
                .instance()
                .set(&DataKey::SmeCount, &(count + 1));
            env.storage()
                .persistent()
                .set(&DataKey::SmeByIndex(count), &sme.clone());
            let scoring_config = load_scoring_config(env);
            CreditScoreData {
                sme: sme.clone(),
                score: scoring_config.core.min_score,
                total_invoices: 0,
                paid_on_time: 0,
                paid_late: 0,
                defaulted: 0,
                total_volume: 0,
                average_payment_days: 0,
                last_updated: env.ledger().timestamp(),
                score_version: scoring_config.core.score_version,
            }
        }
    }

    fn require_admin(env: &Env, admin: &Address) {
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .expect("not initialized");
        if admin != &stored_admin {
            panic_with_error!(env, CreditScoreError::Unauthorized);
        }
    }

    /// Set the upgrade timelock duration in seconds (#338).
    /// Minimum: 3,600 s (1 h). Default: 86,400 s (24 h).
    pub fn set_upgrade_timelock(env: Env, admin: Address, secs: u64) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        if secs < MIN_UPGRADE_TIMELOCK_SECS {
            panic_with_error!(&env, CreditScoreError::InvalidUpgradeTimelock);
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
        env.storage()
            .instance()
            .get(&DataKey::UpgradeTimelockSecs)
            .unwrap_or(UPGRADE_TIMELOCK_SECS)
    }

    pub fn propose_upgrade(env: Env, admin: Address, wasm_hash: BytesN<32>) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
        // #340: reject all-zero hash
        if wasm_hash == BytesN::from_array(&env, &[0u8; 32]) {
            panic_with_error!(&env, CreditScoreError::InvalidWasmHash);
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
        Self::require_admin(&env, &admin);
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
            panic_with_error!(&env, CreditScoreError::UpgradeTimelockNotExpired);
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
    pub fn propose_admin_change(env: Env, admin: Address, new_admin: Address) {
        admin.require_auth();
        require_not_paused(&env);
        Self::require_admin(&env, &admin);
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
        Self::require_admin(&env, &admin);
        let maybe_scheduled: Option<u64> = env
            .storage()
            .instance()
            .get(&DataKey::AdminChangeScheduledAt);
        let scheduled_at = match maybe_scheduled {
            Some(v) => v,
            None => panic_with_error!(&env, CreditScoreError::NoAdminChangeProposed),
        };
        let now = env.ledger().timestamp();
        if now < scheduled_at + ADMIN_CHANGE_TIMELOCK_SECS {
            panic_with_error!(&env, CreditScoreError::AdminChangeTimelockNotExpired);
        }
        let maybe_new_admin: Option<Address> = env.storage().instance().get(&DataKey::PendingAdmin);
        let new_admin = match maybe_new_admin {
            Some(v) => v,
            None => panic_with_error!(&env, CreditScoreError::NoAdminChangeProposed),
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
    pub fn cancel_admin_change(env: Env, admin: Address) {
        admin.require_auth();
        require_not_paused(&env);
        Self::require_admin(&env, &admin);
        if !env.storage().instance().has(&DataKey::PendingAdmin) {
            panic_with_error!(&env, CreditScoreError::NoAdminChangeProposed);
        }
        env.storage().instance().remove(&DataKey::PendingAdmin);
        env.storage()
            .instance()
            .remove(&DataKey::AdminChangeScheduledAt);
        env.events()
            .publish((EVT, Symbol::new(&env, "admin_chg_cancelled")), admin);
    }
}

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{
        testutils::Address as _, testutils::Events, testutils::Ledger, Env, IntoVal,
    };

    fn setup(env: &Env) -> (CreditScoreContractClient<'_>, Address, Address, Address) {
        let contract_id = env.register(CreditScoreContract, ());
        let client = CreditScoreContractClient::new(env, &contract_id);
        let admin = Address::generate(env);
        let invoice_contract = Address::generate(env);
        let pool_contract = Address::generate(env);
        client.initialize(&admin, &invoice_contract, &pool_contract);
        (client, admin, invoice_contract, pool_contract)
    }

    #[test]
    fn test_initial_score() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, _invoice, _pool) = setup(&env);
        let sme = Address::generate(&env);

        let score_data = client.get_credit_score(&sme);
        assert_eq!(score_data.score, MIN_SCORE);
        assert_eq!(score_data.total_invoices, 0);
    }

    #[test]
    fn test_record_payment_on_time() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        let due_date = 200_000u64;
        let paid_at = 150_000u64;

        client.record_payment(&pool, &1, &sme, &1_000_000_000i128, &due_date, &paid_at);

        let score_data = client.get_credit_score(&sme);
        assert_eq!(score_data.total_invoices, 1);
        assert_eq!(score_data.paid_on_time, 1);
        assert_eq!(score_data.paid_late, 0);
        assert_eq!(score_data.defaulted, 0);
        assert!(score_data.score > MIN_SCORE);
    }

    #[test]
    fn test_record_payment_late() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        let due_date = 100_000u64;
        let paid_at = 150_000u64;

        client.record_payment(&pool, &1, &sme, &1_000_000_000i128, &due_date, &paid_at);

        let score_data = client.get_credit_score(&sme);
        assert_eq!(score_data.total_invoices, 1);
        assert_eq!(score_data.paid_on_time, 0);
        assert_eq!(score_data.paid_late, 1);
        assert!(score_data.score > MIN_SCORE);
    }

    #[test]
    fn test_record_default() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 200_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        let due_date = 100_000u64;

        client.record_default(&pool, &1, &sme, &1_000_000_000i128, &due_date);

        let score_data = client.get_credit_score(&sme);
        assert_eq!(score_data.total_invoices, 1);
        assert_eq!(score_data.defaulted, 1);
        assert!(score_data.score < BASE_SCORE);
    }

    #[test]
    fn test_multiple_payments_improve_score() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        let due_date = 200_000u64;

        for i in 1..=10 {
            client.record_payment(
                &pool,
                &i,
                &sme,
                &1_000_000_000i128,
                &due_date,
                &(due_date - 1000),
            );
        }

        let score_data = client.get_credit_score(&sme);
        assert_eq!(score_data.total_invoices, 10);
        assert_eq!(score_data.paid_on_time, 10);
        assert!(score_data.score > BASE_SCORE);
    }

    #[test]
    fn test_defaults_decrease_score() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 300_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        let due_date = 100_000u64;

        client.record_payment(
            &pool,
            &1,
            &sme,
            &1_000_000_000i128,
            &due_date,
            &(due_date - 1000),
        );
        client.record_default(&pool, &2, &sme, &1_000_000_000i128, &due_date);
        client.record_default(&pool, &3, &sme, &1_000_000_000i128, &due_date);

        let score_data = client.get_credit_score(&sme);
        assert_eq!(score_data.total_invoices, 3);
        assert_eq!(score_data.paid_on_time, 1);
        assert_eq!(score_data.defaulted, 2);
        assert!(score_data.score < BASE_SCORE);
    }

    #[test]
    fn test_payment_history() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        let due_date = 200_000u64;

        client.record_payment(
            &pool,
            &1,
            &sme,
            &1_000_000_000i128,
            &due_date,
            &(due_date - 1000),
        );
        client.record_payment(&pool, &2, &sme, &2_000_000_000i128, &due_date, &due_date);
        client.record_default(&pool, &3, &sme, &500_000_000i128, &due_date);

        let history = client.get_payment_history(&sme);
        assert_eq!(history.len(), 3);

        let record1 = client.get_payment_record(&sme, &0).unwrap();
        assert_eq!(record1.invoice_id, 1);
        assert!(matches!(record1.status, PaymentStatus::PaidOnTime));

        let record2 = client.get_payment_record(&sme, &1).unwrap();
        assert_eq!(record2.invoice_id, 2);
        assert!(matches!(record2.status, PaymentStatus::PaidOnTime));

        let record3 = client.get_payment_record(&sme, &2).unwrap();
        assert_eq!(record3.invoice_id, 3);
        assert!(matches!(record3.status, PaymentStatus::Defaulted));
    }

    #[test]
    fn test_cannot_process_same_invoice_twice() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        let due_date = 200_000u64;

        client.record_payment(
            &pool,
            &1,
            &sme,
            &1_000_000_000i128,
            &due_date,
            &(due_date - 1000),
        );

        let result = client.try_record_payment(
            &pool,
            &1,
            &sme,
            &1_000_000_000i128,
            &due_date,
            &(due_date - 1000),
        );
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::InvoiceAlreadyProcessed.into()
        );
    }

    #[test]
    fn test_score_bands() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, _invoice, _pool) = setup(&env);

        // Test with default thresholds: excellent=800, very_good=740, good=670, fair=580
        assert_eq!(
            client.get_score_band(&850),
            String::from_str(&env, "Excellent")
        );
        assert_eq!(
            client.get_score_band(&800),
            String::from_str(&env, "Excellent")
        );
        assert_eq!(
            client.get_score_band(&750),
            String::from_str(&env, "Very Good")
        );
        assert_eq!(client.get_score_band(&700), String::from_str(&env, "Good"));
        assert_eq!(client.get_score_band(&650), String::from_str(&env, "Fair"));
        assert_eq!(client.get_score_band(&600), String::from_str(&env, "Fair"));
        assert_eq!(client.get_score_band(&550), String::from_str(&env, "Poor"));
        assert_eq!(
            client.get_score_band(&400),
            String::from_str(&env, "Very Poor")
        );
    }

    #[test]
    fn test_set_score_thresholds() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, _invoice, _pool) = setup(&env);

        // Test default thresholds
        let defaults = ScoreThresholds::defaults();
        assert_eq!(defaults.excellent, 800);
        assert_eq!(defaults.very_good, 740);
        assert_eq!(defaults.good, 670);
        assert_eq!(defaults.fair, 580);

        // Update to new thresholds
        let new_thresholds = ScoreThresholds {
            excellent: 750,
            very_good: 700,
            good: 650,
            fair: 600,
        };
        client.set_score_thresholds(&admin, &new_thresholds);

        // Verify new thresholds are applied
        assert_eq!(
            client.get_score_band(&750),
            String::from_str(&env, "Excellent")
        );
        assert_eq!(
            client.get_score_band(&700),
            String::from_str(&env, "Very Good")
        );
        assert_eq!(client.get_score_band(&650), String::from_str(&env, "Good"));
        assert_eq!(client.get_score_band(&600), String::from_str(&env, "Fair"));
    }

    #[test]
    fn test_set_invalid_score_thresholds() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, _invoice, _pool) = setup(&env);

        let invalid_thresholds = ScoreThresholds {
            excellent: 700,
            very_good: 750, // Invalid: greater than excellent
            good: 650,
            fair: 600,
        };
        let result = client.try_set_score_thresholds(&admin, &invalid_thresholds);
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::InvalidThresholds.into()
        );
    }

    #[test]
    fn test_invoice_processed_check() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        assert!(!client.is_invoice_processed(&1));

        let due_date = 200_000u64;
        client.record_payment(
            &pool,
            &1,
            &sme,
            &1_000_000_000i128,
            &due_date,
            &(due_date - 1000),
        );

        assert!(client.is_invoice_processed(&1));
    }

    // **Feature: credit-scoring, Property 1: Score bounds invariant**
    // **Validates: Requirements 1.5, 1.6**
    #[test]
    fn test_prop_score_bounds_invariant() {
        // For any combination of inputs, score must always be in [MIN_SCORE, MAX_SCORE].
        // Uses a simple LCG to generate 100 varied input combinations.
        let env = Env::default();
        let contract_id = env.register(CreditScoreContract, ());
        let client = CreditScoreContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let _invoice = Address::generate(&env);
        let _pool = Address::generate(&env);
        client.initialize(&admin, &_invoice, &_pool);
        let mut seed: u64 = 0xDEAD_BEEF_1234_5678;
        let lcg = |s: &mut u64| -> u64 {
            *s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *s
        };

        env.as_contract(&contract_id, || {
        for _ in 0..100 {
            let total_invoices = (lcg(&mut seed) % 50 + 1) as u32;
            let paid_on_time = (lcg(&mut seed) % (total_invoices as u64 + 1)) as u32;
            let remaining = total_invoices - paid_on_time;
            let paid_late = (lcg(&mut seed) % (remaining as u64 + 1)) as u32;
            let defaulted = remaining - paid_late;
            let total_volume = (lcg(&mut seed) % 200_000_000_000) as i128;
            let avg_days = (lcg(&mut seed) % 60) as i64 - 10; // -10 to +49

            let score = calculate_score(
                &env,
                30,
                total_invoices,
                paid_on_time,
                paid_late,
                defaulted,
                total_volume,
                avg_days,
            );
            assert!(
                score >= MIN_SCORE && score <= MAX_SCORE,
                "score {} out of bounds [{}, {}] for inputs: total={} on_time={} late={} defaulted={} vol={} avg_days={}",
                score, MIN_SCORE, MAX_SCORE, total_invoices, paid_on_time, paid_late, defaulted, total_volume, avg_days
            );
            }
        });
    }

    // **Feature: credit-scoring, Property 2: Scoring formula monotonicity**
    // **Validates: Requirements 1.2, 1.3, 1.4**
    #[test]
    fn test_prop_scoring_formula_monotonicity() {
        // For any fixed base, adding an on-time payment scores >= adding a late payment
        // which scores >= adding a default.
        let env = Env::default();
        let contract_id = env.register(CreditScoreContract, ());
        let client = CreditScoreContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let _invoice = Address::generate(&env);
        let _pool = Address::generate(&env);
        client.initialize(&admin, &_invoice, &_pool);
        let mut seed: u64 = 0xCAFE_BABE_0000_0001;
        let lcg = |s: &mut u64| -> u64 {
            *s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *s
        };

        env.as_contract(&contract_id, || {
            for _ in 0..100 {
                let base_invoices = (lcg(&mut seed) % 19 + 1) as u32;
                let base_on_time = (lcg(&mut seed) % (base_invoices as u64 + 1)) as u32;
                let base_remaining = base_invoices - base_on_time;
                let base_late = (lcg(&mut seed) % (base_remaining as u64 + 1)) as u32;
                let base_defaulted = base_remaining - base_late;
                let vol = (lcg(&mut seed) % 50_000_000_000) as i128;
                let avg = (lcg(&mut seed) % 20) as i64;

                let score_on_time = calculate_score(
                    &env,
                    30,
                    base_invoices + 1,
                    base_on_time + 1,
                    base_late,
                    base_defaulted,
                    vol,
                    avg,
                );
                let score_late = calculate_score(
                    &env,
                    30,
                    base_invoices + 1,
                    base_on_time,
                    base_late + 1,
                    base_defaulted,
                    vol,
                    avg,
                );
                let score_default = calculate_score(
                    &env,
                    30,
                    base_invoices + 1,
                    base_on_time,
                    base_late,
                    base_defaulted + 1,
                    vol,
                    avg,
                );

                assert!(
                    score_on_time >= score_late,
                    "on_time score {} < late score {} — monotonicity violated",
                    score_on_time,
                    score_late
                );
                assert!(
                    score_late >= score_default,
                    "late score {} < default score {} — monotonicity violated",
                    score_late,
                    score_default
                );
            }
        });
    }

    // **Feature: credit-scoring, Property 3: Defaults dominate — score below BASE when defaults exceed on-time**
    // **Validates: Requirements 4.1**
    #[test]
    fn test_prop_defaults_dominate() {
        // When defaulted > paid_on_time and paid_late == 0, score must be < BASE_SCORE.
        let env = Env::default();
        let contract_id = env.register(CreditScoreContract, ());
        let client = CreditScoreContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let _invoice = Address::generate(&env);
        let _pool = Address::generate(&env);
        client.initialize(&admin, &_invoice, &_pool);
        let mut seed: u64 = 0xF00D_CAFE_ABCD_EF01;
        let lcg = |s: &mut u64| -> u64 {
            *s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *s
        };

        env.as_contract(&contract_id, || {
            for _ in 0..100 {
                let on_time = (lcg(&mut seed) % 10) as u32;
                let defaulted = on_time + (lcg(&mut seed) % 10 + 1) as u32; // always > on_time
                let total = on_time + defaulted;
                let vol = (lcg(&mut seed) % 5_000_000_000) as i128;
                let avg = (lcg(&mut seed) % 15) as i64;

                let score = calculate_score(&env, 30, total, on_time, 0, defaulted, vol, avg);
                assert!(
                score < BASE_SCORE,
                "score {} >= BASE_SCORE {} when defaulted({}) > on_time({}) with no late payments",
                score,
                BASE_SCORE,
                defaulted,
                on_time
            );
            }
        });
    }

    // **Feature: credit-scoring, Property 7: Score band coverage**
    // **Validates: Requirements 6.1, 6.2, 6.3, 6.4, 6.5, 6.6**
    #[test]
    fn test_prop_score_band_coverage() {
        // For every score in [MIN_SCORE, MAX_SCORE], get_score_band returns the correct band.
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _invoice, _pool) = setup(&env);

        // Get the thresholds being used
        let thresholds = ScoreThresholds::defaults();

        for score in MIN_SCORE..=MAX_SCORE {
            let band = client.get_score_band(&score);
            let expected = if score >= thresholds.excellent {
                "Excellent"
            } else if score >= thresholds.very_good {
                "Very Good"
            } else if score >= thresholds.good {
                "Good"
            } else if score >= thresholds.fair {
                "Fair"
            } else if score >= BASE_SCORE {
                "Poor"
            } else {
                "Very Poor"
            };
            assert_eq!(
                band,
                soroban_sdk::String::from_str(&env, expected),
                "score {} should map to '{}' but got '{:?}'",
                score,
                expected,
                band
            );
        }
    }

    // **Feature: credit-scoring, Property 8: Payment history ordering invariant**
    // **Validates: Requirements 7.1, 7.2, 7.3**
    #[test]
    fn test_prop_payment_history_ordering() {
        // For any sequence of N records, get_payment_history returns the most recent
        // MAX_PAYMENT_HISTORY records in insertion order, and get_payment_record(i)
        // matches get_payment_history()[i].
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let mut seed: u64 = 0x0F0F_0F0F_A5A5_A5A5;
        let lcg = |s: &mut u64| -> u64 {
            *s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *s
        };

        for trial in 0..20u64 {
            let (client, _admin, _invoice, pool) = setup(&env);
            let sme = Address::generate(&env);
            let n = (lcg(&mut seed) % 150 + 1) as u64;
            let due_date = 200_000u64;
            let mut expected_ids: std::vec::Vec<u64> = std::vec::Vec::new();

            for i in 0..n {
                let invoice_id = trial * 20 + i + 1;
                if expected_ids.len() == MAX_PAYMENT_HISTORY as usize {
                    expected_ids.remove(0);
                }
                expected_ids.push(invoice_id);
                let is_default = lcg(&mut seed) % 3 == 0;
                if is_default {
                    client.record_default(&pool, &invoice_id, &sme, &1_000_000_000i128, &due_date);
                } else {
                    client.record_payment(
                        &pool,
                        &invoice_id,
                        &sme,
                        &1_000_000_000i128,
                        &due_date,
                        &(due_date - 1000),
                    );
                }
            }

            let expected_len = n.min(MAX_PAYMENT_HISTORY as u64) as u32;

            // History length matches the capped window size.
            assert_eq!(
                client.get_payment_history_length(&sme),
                expected_len,
                "trial {}: history length mismatch",
                trial
            );

            // Full history in order
            let history = client.get_payment_history(&sme);
            assert_eq!(
                history.len(),
                expected_len,
                "trial {}: history vec length mismatch",
                trial
            );

            // Individual record lookup matches history
            for i in 0..expected_len {
                let by_index = client.get_payment_record(&sme, &i).unwrap();
                let from_history = history.get(i).unwrap();
                assert_eq!(
                    by_index.invoice_id, from_history.invoice_id,
                    "trial {}: record {} invoice_id mismatch",
                    trial, i
                );
                assert_eq!(
                    by_index.invoice_id,
                    *expected_ids.get(i as usize).unwrap(),
                    "trial {}: record {} not in insertion order",
                    trial,
                    i
                );
            }
        }
    }

    #[test]
    fn test_payment_history_rolling_window_caps_at_100() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);
        let due_date = 200_000u64;

        for invoice_id in 1u64..=200u64 {
            client.record_payment(
                &pool,
                &invoice_id,
                &sme,
                &1_000_000_000i128,
                &due_date,
                &(due_date - 1000),
            );
        }

        assert_eq!(client.get_payment_history_length(&sme), MAX_PAYMENT_HISTORY);

        let history = client.get_payment_history(&sme);
        assert_eq!(history.len(), MAX_PAYMENT_HISTORY);
        assert_eq!(history.get(0).unwrap().invoice_id, 101);
        assert_eq!(history.get(99).unwrap().invoice_id, 200);
        assert_eq!(client.get_payment_record(&sme, &0).unwrap().invoice_id, 101);
        assert_eq!(
            client.get_payment_record(&sme, &99).unwrap().invoice_id,
            200
        );
        assert!(client.get_payment_record(&sme, &100).is_none());
    }

    // **Feature: credit-scoring, Property 9: Idempotency guard**
    // **Validates: Requirements 4.3**
    // Three separate should_panic tests cover all duplicate-processing paths.
    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_prop_idempotency_duplicate_payment() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);
        let due_date = 200_000u64;
        client.record_payment(
            &pool,
            &99,
            &sme,
            &1_000_000_000i128,
            &due_date,
            &(due_date - 1000),
        );
        client.record_payment(
            &pool,
            &99,
            &sme,
            &1_000_000_000i128,
            &due_date,
            &(due_date - 1000),
        );
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_prop_idempotency_duplicate_default() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);
        let due_date = 200_000u64;
        client.record_default(&pool, &98, &sme, &1_000_000_000i128, &due_date);
        client.record_default(&pool, &98, &sme, &1_000_000_000i128, &due_date);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #4)")]
    fn test_prop_idempotency_payment_then_default() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);
        let due_date = 200_000u64;
        client.record_payment(
            &pool,
            &97,
            &sme,
            &1_000_000_000i128,
            &due_date,
            &(due_date - 1000),
        );
        client.record_default(&pool, &97, &sme, &1_000_000_000i128, &due_date);
    }

    #[test]
    fn test_prop_invoice_count_accumulation() {
        // For any sequence of N record_payment/record_default calls, total_invoices == N.
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let mut seed: u64 = 0x1234_5678_9ABC_DEF0;
        let lcg = |s: &mut u64| -> u64 {
            *s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *s
        };

        for trial in 0..20u64 {
            let (client, _admin, _invoice, pool) = setup(&env);
            let sme = Address::generate(&env);
            let n = (lcg(&mut seed) % 15 + 1) as u64; // 1..=15 invoices per trial
            let due_date = 200_000u64;

            for i in 0..n {
                let invoice_id = trial * 100 + i + 1;
                let is_default = lcg(&mut seed) % 3 == 0;
                if is_default {
                    client.record_default(&pool, &invoice_id, &sme, &1_000_000_000i128, &due_date);
                } else {
                    client.record_payment(
                        &pool,
                        &invoice_id,
                        &sme,
                        &1_000_000_000i128,
                        &due_date,
                        &(due_date - 1000),
                    );
                }
            }

            let data = client.get_credit_score(&sme);
            assert_eq!(
                data.total_invoices, n as u32,
                "trial {}: expected total_invoices={} got {}",
                trial, n, data.total_invoices
            );
        }
    }

    // **Feature: credit-scoring, Property 5: Volume accumulation invariant**
    // **Validates: Requirements 3.4**
    #[test]
    fn test_prop_volume_accumulation() {
        // For any sequence of payments/defaults with amounts a1..aN, total_volume == sum(ai).
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let mut seed: u64 = 0xABCD_EF01_2345_6789;
        let lcg = |s: &mut u64| -> u64 {
            *s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *s
        };

        for trial in 0..20u64 {
            let (client, _admin, _invoice, pool) = setup(&env);
            let sme = Address::generate(&env);
            let n = (lcg(&mut seed) % 10 + 1) as u64;
            let due_date = 200_000u64;
            let mut expected_volume: i128 = 0;

            for i in 0..n {
                let invoice_id = trial * 50 + i + 1;
                let amount = (lcg(&mut seed) % 5_000_000_000 + 1_000_000) as i128;
                expected_volume += amount;
                let is_default = lcg(&mut seed) % 4 == 0;
                if is_default {
                    client.record_default(&pool, &invoice_id, &sme, &amount, &due_date);
                } else {
                    client.record_payment(
                        &pool,
                        &invoice_id,
                        &sme,
                        &amount,
                        &due_date,
                        &(due_date - 1000),
                    );
                }
            }

            let data = client.get_credit_score(&sme);
            assert_eq!(
                data.total_volume, expected_volume,
                "trial {}: expected total_volume={} got {}",
                trial, expected_volume, data.total_volume
            );
        }
    }

    #[test]
    fn test_total_volume_tracking() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        let due_date = 200_000u64;

        client.record_payment(
            &pool,
            &1,
            &sme,
            &1_000_000_000i128,
            &due_date,
            &(due_date - 1000),
        );
        client.record_payment(
            &pool,
            &2,
            &sme,
            &2_000_000_000i128,
            &due_date,
            &(due_date - 1000),
        );
        client.record_payment(
            &pool,
            &3,
            &sme,
            &3_000_000_000i128,
            &due_date,
            &(due_date - 1000),
        );

        let score_data = client.get_credit_score(&sme);
        assert_eq!(score_data.total_volume, 6_000_000_000i128);
    }

    // ---- Circuit Breaker Tests ----

    #[test]
    fn test_credit_is_paused_false_after_init() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _inv, _pool) = setup(&env);
        assert!(!client.is_paused());
    }

    #[test]
    fn test_credit_pause_and_unpause() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _inv, _pool) = setup(&env);
        client.pause(&admin);
        assert!(client.is_paused());
        client.unpause(&admin);
        assert!(!client.is_paused());
    }

    #[test]
    fn test_credit_pause_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _inv, _pool) = setup(&env);
        let intruder = Address::generate(&env);
        let result = client.try_pause(&intruder);
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::Unauthorized.into()
        );
    }

    #[test]
    fn test_credit_unpause_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _inv, _pool) = setup(&env);
        client.pause(&admin);
        let intruder = Address::generate(&env);
        let result = client.try_unpause(&intruder);
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::Unauthorized.into()
        );
    }

    #[test]
    fn test_record_payment_while_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let (client, admin, _inv, pool) = setup(&env);
        let sme = Address::generate(&env);
        client.pause(&admin);
        let result =
            client.try_record_payment(&pool, &1, &sme, &1_000i128, &200_000u64, &150_000u64);
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::ContractPaused.into()
        );
    }

    #[test]
    fn test_record_default_while_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 200_000);
        let (client, admin, _inv, pool) = setup(&env);
        let sme = Address::generate(&env);
        client.pause(&admin);
        let result = client.try_record_default(&pool, &1, &sme, &1_000i128, &100_000u64);
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::ContractPaused.into()
        );
    }

    #[test]
    fn test_credit_views_succeed_while_paused() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let (client, admin, _inv, pool) = setup(&env);
        let sme = Address::generate(&env);
        client.record_payment(&pool, &1, &sme, &1_000i128, &200_000u64, &150_000u64);
        client.pause(&admin);

        let _ = client.get_credit_score(&sme);
        let _ = client.get_payment_history(&sme);
        let _ = client.get_payment_history_length(&sme);
        let _ = client.get_score_band(&500);
        let _ = client.is_invoice_processed(&1);
        let _ = client.get_config();
        assert!(client.is_paused());
    }

    #[test]
    fn test_credit_pause_unpause_restores_operations() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let (client, admin, _inv, pool) = setup(&env);
        let sme = Address::generate(&env);

        client.pause(&admin);
        client.unpause(&admin);

        client.record_payment(&pool, &1, &sme, &1_000i128, &200_000u64, &150_000u64);
        let data = client.get_credit_score(&sme);
        assert_eq!(data.total_invoices, 1);
    }

    // ---- Issue #404: shared helper exercised via both public entry points ----

    #[test]
    fn test_shared_helper_payment_and_default_same_sme() {
        // Both record_payment and record_default must persist via commit_payment_record
        // and produce consistent credit data.
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);
        let due_date = 200_000u64;

        // Via record_payment (on time)
        client.record_payment(
            &pool,
            &1,
            &sme,
            &1_000_000_000i128,
            &due_date,
            &(due_date - 1000),
        );
        let after_payment = client.get_credit_score(&sme);
        assert_eq!(after_payment.total_invoices, 1);
        assert_eq!(after_payment.paid_on_time, 1);
        assert_eq!(after_payment.defaulted, 0);

        // Via record_default
        client.record_default(&pool, &2, &sme, &1_000_000_000i128, &due_date);
        let after_default = client.get_credit_score(&sme);
        assert_eq!(after_default.total_invoices, 2);
        assert_eq!(after_default.paid_on_time, 1);
        assert_eq!(after_default.defaulted, 1);

        // History length reflects both writes
        assert_eq!(client.get_payment_history_length(&sme), 2);
    }

    // ---- Issue #405: calculate_average_payment_days divide-by-zero guard ----

    #[test]
    fn test_average_payment_days_empty_history_returns_zero() {
        // New borrower with no payment records must return 0, not panic.
        let result = calculate_average_payment_days(0, 0, 0);
        assert_eq!(result, 0, "empty history must return 0");
    }

    #[test]
    fn test_average_payment_days_single_on_time_payment() {
        // One on-time payment with 0 late days → average is 0.
        let result = calculate_average_payment_days(1, 0, 0);
        assert_eq!(result, 0);
    }

    #[test]
    fn test_average_payment_days_single_late_payment() {
        // One late payment with 5 days late → average is 5.
        let result = calculate_average_payment_days(0, 1, 5);
        assert_eq!(result, 5);
    }

    #[test]
    fn test_average_payment_days_new_borrower_no_panic() {
        // Fuzz: zero-length and one-element inputs must never panic.
        for paid_on_time in 0u32..=1 {
            for paid_late in 0u32..=1 {
                let total_late: i64 = (paid_late as i64) * 3;
                let result = calculate_average_payment_days(paid_on_time, paid_late, total_late);
                let total_paid = paid_on_time + paid_late;
                if total_paid == 0 {
                    assert_eq!(result, 0, "zero-length must return 0");
                } else {
                    assert_eq!(result, total_late / total_paid as i64);
                }
            }
        }
    }

    // ---- Issue #61: Edge-Case Tests ----

    #[test]
    fn test_score_floor_never_below_200() {
        // Mass defaults must never push score below MIN_SCORE (200)
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 300_000);
        let (client, _admin, _inv, pool) = setup(&env);
        let sme = Address::generate(&env);
        let due_date = 100_000u64;

        for i in 1..=50u64 {
            client.record_default(&pool, &i, &sme, &1_000_000_000i128, &due_date);
        }

        let data = client.get_credit_score(&sme);
        assert!(
            data.score >= MIN_SCORE,
            "score {} dropped below floor {}",
            data.score,
            MIN_SCORE
        );
    }

    #[test]
    fn test_score_ceiling_never_above_850() {
        // Perfect payment history must never push score above MAX_SCORE (850)
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let (client, _admin, _inv, pool) = setup(&env);
        let sme = Address::generate(&env);
        let due_date = 200_000u64;

        for i in 1..=50u64 {
            // Pay early to maximize score
            client.record_payment(
                &pool,
                &i,
                &sme,
                &100_000_000_000i128,
                &due_date,
                &(due_date - 86_400),
            );
        }

        let data = client.get_credit_score(&sme);
        assert!(
            data.score <= MAX_SCORE,
            "score {} exceeded ceiling {}",
            data.score,
            MAX_SCORE
        );
    }

    #[test]
    fn test_score_band_classification() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _inv, _pool) = setup(&env);

        assert_eq!(
            client.get_score_band(&MIN_SCORE),
            String::from_str(&env, "Very Poor")
        );
        assert_eq!(
            client.get_score_band(&MAX_SCORE),
            String::from_str(&env, "Excellent")
        );
        assert_eq!(client.get_score_band(&500), String::from_str(&env, "Poor"));
        assert_eq!(client.get_score_band(&580), String::from_str(&env, "Fair"));
        assert_eq!(client.get_score_band(&670), String::from_str(&env, "Good"));
        assert_eq!(
            client.get_score_band(&740),
            String::from_str(&env, "Very Good")
        );
        assert_eq!(
            client.get_score_band(&800),
            String::from_str(&env, "Excellent")
        );
    }

    #[test]
    fn test_default_does_not_affect_average_payment_days() {
        // Defaults must not be included in average_payment_days calculation
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let (client, _admin, _inv, pool) = setup(&env);
        let sme = Address::generate(&env);
        let due_date = 200_000u64;

        // One on-time payment
        client.record_payment(&pool, &1, &sme, &1_000i128, &due_date, &(due_date - 1000));
        let data_before = client.get_credit_score(&sme);

        // Add a default — must not change average_payment_days
        client.record_default(&pool, &2, &sme, &1_000i128, &due_date);
        let data_after = client.get_credit_score(&sme);

        assert_eq!(
            data_before.average_payment_days, data_after.average_payment_days,
            "default must not affect average_payment_days"
        );
    }

    // ---- days_late ceiling division tests ----

    #[test]
    fn test_days_late_ceil_one_hour() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        let due_date = 200_000u64;
        let paid_at = 200_000u64 + 3600; // 1 hour late

        client.record_payment(&pool, &1, &sme, &1_000_000_000i128, &due_date, &paid_at);

        let record = client.get_payment_record(&sme, &0).unwrap();
        assert_eq!(
            record.days_late, 1,
            "1 hour late should be 1 day late (ceiling)"
        );
    }

    #[test]
    fn test_days_late_ceil_twenty_five_hours() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        let due_date = 200_000u64;
        let paid_at = 200_000u64 + 90_000; // 25 hours = 90000 seconds

        client.record_payment(&pool, &1, &sme, &1_000_000_000i128, &due_date, &paid_at);

        let record = client.get_payment_record(&sme, &0).unwrap();
        assert_eq!(
            record.days_late, 2,
            "25 hours late should be 2 days late (ceiling)"
        );
    }

    #[test]
    fn test_days_late_on_time_is_zero_or_negative() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        let due_date = 200_000u64;

        // Exact on-time
        client.record_payment(&pool, &1, &sme, &1_000_000_000i128, &due_date, &due_date);
        let r1 = client.get_payment_record(&sme, &0).unwrap();
        assert!(
            r1.days_late <= 0,
            "on-time payment must have days_late <= 0, got {}",
            r1.days_late
        );

        // Early
        client.record_payment(
            &pool,
            &2,
            &sme,
            &1_000_000_000i128,
            &due_date,
            &(due_date - 1000),
        );
        let r2 = client.get_payment_record(&sme, &1).unwrap();
        assert!(
            r2.days_late <= 0,
            "early payment must have days_late <= 0, got {}",
            r2.days_late
        );
    }

    #[test]
    fn test_default_days_late_uses_ceiling() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 300_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        let due_date = 100_000u64;

        // Default at 1 hour past due
        env.ledger().with_mut(|l| l.timestamp = due_date + 3600);
        client.record_default(&pool, &1, &sme, &1_000_000_000i128, &due_date);

        let record = client.get_payment_record(&sme, &0).unwrap();
        assert_eq!(
            record.days_late, 1,
            "default 1 hour late should be 1 day (ceiling)"
        );
    }

    #[test]
    #[should_panic]
    fn test_unauthorized_record_payment_panics() {
        // A random address that is not the pool must fail require_auth
        let env = Env::default();
        // No mock_all_auths — auth checks are enforced
        let (client, _admin, _inv, _pool) = setup(&env);
        let sme = Address::generate(&env);
        let attacker = Address::generate(&env);
        client.record_payment(&attacker, &1, &sme, &1_000i128, &200_000u64, &150_000u64);
    }

    // ---- Issue #430: Configurable late-payment threshold ----

    #[test]
    fn test_late_threshold_default_is_30() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _inv, _pool) = setup(&env);
        assert_eq!(client.get_late_threshold(), 30);
    }

    #[test]
    fn test_set_late_threshold_updates_value() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _inv, _pool) = setup(&env);
        client.set_late_threshold(&admin, &60);
        assert_eq!(client.get_late_threshold(), 60);
    }

    #[test]
    fn test_set_late_threshold_rejects_zero() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _inv, _pool) = setup(&env);
        let result = client.try_set_late_threshold(&admin, &0);
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::InvalidLateThreshold.into()
        );
    }

    #[test]
    fn test_set_late_threshold_rejects_over_365() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _inv, _pool) = setup(&env);
        let result = client.try_set_late_threshold(&admin, &366);
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::InvalidLateThreshold.into()
        );
    }

    #[test]
    fn test_set_late_threshold_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _inv, _pool) = setup(&env);
        let intruder = Address::generate(&env);
        let result = client.try_set_late_threshold(&intruder, &45);
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::Unauthorized.into()
        );
    }

    #[test]
    fn test_late_threshold_affects_score() {
        // With threshold=1, avg_payment_days=5 should trigger the penalty.
        // With threshold=60, avg_payment_days=5 should NOT trigger the penalty.
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, admin, _inv, pool) = setup(&env);
        let sme1 = Address::generate(&env);
        let sme2 = Address::generate(&env);

        // sme1: threshold=1 (8 days late > 1 → penalty)
        client.set_late_threshold(&admin, &1);
        let due = 200_000u64;
        // Exactly 7 days late → still PaidLate (≤7 day threshold), but days_late=8
        // so avg_days=8 which is >7 and enters the late_threshold penalty branch
        let paid_late = due + 7 * 86_400;
        client.record_payment(&pool, &1, &sme1, &1_000_000_000i128, &due, &paid_late);
        let score_strict = client.get_credit_score(&sme1).score;

        // sme2: threshold=60 (8 days late ≤ 60 → no penalty)
        client.set_late_threshold(&admin, &60);
        client.record_payment(&pool, &2, &sme2, &1_000_000_000i128, &due, &paid_late);
        let score_lenient = client.get_credit_score(&sme2).score;

        assert!(
            score_lenient > score_strict,
            "lenient threshold should yield higher score: lenient={} strict={}",
            score_lenient,
            score_strict
        );
    }

    #[test]
    fn test_run_migration_bumps_version_and_is_idempotent() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _invoice, _pool) = setup(&env);

        // Fresh deployment starts at migration level 0.
        assert_eq!(client.migration_version(), 0);

        // Running the migration advances to the current schema version while
        // preserving existing state (the contract stays at the same version).
        let version_before = client.version();
        client.run_migration(&admin);
        assert_eq!(client.migration_version(), CURRENT_MIGRATION_VERSION);
        assert_eq!(client.version(), version_before);

        // Re-running is a no-op (idempotent).
        client.run_migration(&admin);
        assert_eq!(client.migration_version(), CURRENT_MIGRATION_VERSION);
    }

    #[test]
    #[should_panic(expected = "Error(Contract, #2)")]
    fn test_run_migration_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _invoice, _pool) = setup(&env);
        let attacker = Address::generate(&env);
        client.run_migration(&attacker);
    }

    // **Feature: credit-scoring, Issue #378: data inconsistency warning event**
    // Verifies that calculate_score emits a warning event when the invoice counters
    // do not sum to total_invoices, while still returning a clamped score.
    #[test]
    fn test_inconsistent_data_emits_warning_event() {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(CreditScoreContract, ());
        let client = CreditScoreContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let _invoice = Address::generate(&env);
        let _pool = Address::generate(&env);
        client.initialize(&admin, &_invoice, &_pool);
        // paid_on_time(3) + paid_late(2) + defaulted(1) = 6, but total_invoices = 10
        let (score, found) = env.as_contract(&contract_id, || {
            let s = calculate_score(&env, 30, 10, 3, 2, 1, 1_000_000_000, 5);
            let events = env.events().all();
            let f = events.len() > 0;
            (s, f)
        });
        assert!(
            score >= MIN_SCORE && score <= MAX_SCORE,
            "score {} out of [{}, {}]",
            score,
            MIN_SCORE,
            MAX_SCORE
        );
        assert!(
            found,
            "data_inconsistency event not found in emitted events"
        );
    }

    // ── #338: configurable upgrade timelock tests ─────────────────────────────

    #[test]
    fn test_credit_score_upgrade_timelock_default_is_24h() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _invoice, _pool) = setup(&env);
        assert_eq!(client.get_upgrade_timelock(), UPGRADE_TIMELOCK_SECS);
    }

    #[test]
    fn test_credit_score_set_upgrade_timelock_configures_value() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _invoice, _pool) = setup(&env);
        client.set_upgrade_timelock(&admin, &7_200u64);
        assert_eq!(client.get_upgrade_timelock(), 7_200u64);
    }

    #[test]
    fn test_credit_score_set_upgrade_timelock_below_minimum_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _invoice, _pool) = setup(&env);
        let result = client.try_set_upgrade_timelock(&admin, &(MIN_UPGRADE_TIMELOCK_SECS - 1));
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::InvalidUpgradeTimelock.into()
        );
    }

    #[test]
    fn test_credit_score_execute_upgrade_before_timelock_fails() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _invoice, _pool) = setup(&env);
        client.set_upgrade_timelock(&admin, &7_200u64);

        let hash = BytesN::from_array(&env, &[1u8; 32]);
        client.propose_upgrade(&admin, &hash);

        env.ledger().with_mut(|l| l.timestamp += 3_600);
        let result = client.try_execute_upgrade(&admin);
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::UpgradeTimelockNotExpired.into()
        );
    }

    // ── #340: WASM hash validation tests ─────────────────────────────────────

    #[test]
    fn test_credit_score_propose_upgrade_zero_hash_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _invoice, _pool) = setup(&env);
        let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
        let result = client.try_propose_upgrade(&admin, &zero_hash);
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::InvalidWasmHash.into()
        );
    }

    #[test]
    fn test_credit_score_propose_upgrade_nonzero_hash_accepted() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _invoice, _pool) = setup(&env);
        let valid_hash = BytesN::from_array(&env, &[7u8; 32]);
        client.propose_upgrade(&admin, &valid_hash);
    }

    // ── #573: config-version staleness detection ──────────────────────────────

    #[test]
    fn test_get_credit_score_returns_config_version() {
        // get_credit_score must expose the current config version so consumers
        // can detect staleness without a separate get_scoring_config() call.
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _invoice, _pool) = setup(&env);
        let sme = Address::generate(&env);

        let resp = client.get_credit_score(&sme);
        // Default config starts at score_version = 1.
        assert_eq!(
            resp.config_version, 1,
            "config_version should be 1 after init"
        );
        assert_eq!(
            resp.score_version, 1,
            "fresh SME score_version should equal config_version"
        );
        assert!(!resp.is_stale, "fresh score must not be stale");
    }

    #[test]
    fn test_score_not_stale_after_payment_under_current_config() {
        // A score computed under the current config is not stale.
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, _admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        client.record_payment(
            &pool,
            &1,
            &sme,
            &1_000_000_000i128,
            &200_000u64,
            &150_000u64,
        );

        let resp = client.get_credit_score(&sme);
        assert_eq!(resp.score_version, resp.config_version);
        assert!(
            !resp.is_stale,
            "score computed under current config must not be stale"
        );
    }

    #[test]
    fn test_score_flagged_stale_after_config_update() {
        // After the scoring config is updated (score_version bumped), any SME whose
        // score was computed under the old config must be flagged as stale.
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        // Record a payment under config v1.
        client.record_payment(
            &pool,
            &1,
            &sme,
            &1_000_000_000i128,
            &200_000u64,
            &150_000u64,
        );
        let before = client.get_credit_score(&sme);
        assert!(
            !before.is_stale,
            "score should be current before config change"
        );
        assert_eq!(before.score_version, 1);
        assert_eq!(before.config_version, 1);

        // Admin updates scoring config, bumping score_version to 2.
        let mut new_config = client.get_scoring_config();
        new_config.core.score_version = 2;
        client.set_scoring_config(&admin, &new_config);

        // The stored score was computed under v1 but the contract now runs v2.
        let after = client.get_credit_score(&sme);
        assert_eq!(
            after.score_version, 1,
            "score_version must reflect when score was computed"
        );
        assert_eq!(
            after.config_version, 2,
            "config_version must reflect current config"
        );
        assert!(
            after.is_stale,
            "score computed under v1 must be stale under v2 config"
        );
    }

    #[test]
    fn test_score_no_longer_stale_after_new_payment_under_new_config() {
        // Once a new payment is recorded under the new config the score is current again.
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);

        let (client, admin, _invoice, pool) = setup(&env);
        let sme = Address::generate(&env);

        // Payment under v1.
        client.record_payment(
            &pool,
            &1,
            &sme,
            &1_000_000_000i128,
            &200_000u64,
            &150_000u64,
        );

        // Bump config to v2.
        let mut new_config = client.get_scoring_config();
        new_config.core.score_version = 2;
        client.set_scoring_config(&admin, &new_config);

        // Confirm stale.
        assert!(client.get_credit_score(&sme).is_stale);

        // New payment recorded under v2 — score is recomputed under the new config.
        client.record_payment(
            &pool,
            &2,
            &sme,
            &1_000_000_000i128,
            &200_000u64,
            &150_000u64,
        );

        let resp = client.get_credit_score(&sme);
        assert_eq!(
            resp.score_version, 2,
            "score_version must advance to new config version"
        );
        assert_eq!(resp.config_version, 2);
        assert!(
            !resp.is_stale,
            "score recomputed under v2 must not be stale"
        );
    }

    #[test]
    fn test_new_sme_always_current_even_after_config_update() {
        // An SME with no history has a synthetic initial record seeded from the
        // current config, so their score_version always matches config_version.
        //
        // This relies on `get_or_create_credit_data` initialising new records with
        // `score_version: scoring_config.core.score_version` (i.e. the version
        // currently stored on-chain), not a hard-coded 1. If that ever changes,
        // every new SME created after a config bump would incorrectly appear stale
        // on first access.
        let env = Env::default();
        env.mock_all_auths();

        let (client, admin, _invoice, _pool) = setup(&env);

        // Bump config to v2 before the SME ever appears.
        let mut new_config = client.get_scoring_config();
        new_config.core.score_version = 2;
        client.set_scoring_config(&admin, &new_config);

        let new_sme = Address::generate(&env);
        let resp = client.get_credit_score(&new_sme);
        // score_version == 2 here because get_or_create_credit_data seeds new
        // records from the active scoring config, not from a constant.
        assert_eq!(resp.config_version, 2);
        assert_eq!(resp.score_version, 2);
        assert!(!resp.is_stale);
    }

    // ── #565: credit_score admin key rotation timelock tests ─────────────────

    #[test]
    fn test_cs_propose_admin_change_stores_pending() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _invoice, _pool) = setup(&env);
        let new_admin = Address::generate(&env);
        client.propose_admin_change(&admin, &new_admin);
    }

    #[test]
    fn test_cs_finalize_admin_change_before_timelock_fails() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _invoice, _pool) = setup(&env);
        let new_admin = Address::generate(&env);
        client.propose_admin_change(&admin, &new_admin);
        let result = client.try_finalize_admin_change(&admin);
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::AdminChangeTimelockNotExpired.into()
        );
    }

    #[test]
    fn test_cs_finalize_admin_change_after_timelock_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _invoice, _pool) = setup(&env);
        let new_admin = Address::generate(&env);
        client.propose_admin_change(&admin, &new_admin);
        env.ledger()
            .with_mut(|l| l.timestamp += ADMIN_CHANGE_TIMELOCK_SECS + 1);
        client.finalize_admin_change(&admin);
        // new_admin can now perform admin-only operations
        client.pause(&new_admin);
    }

    #[test]
    fn test_cs_cancel_admin_change_removes_pending() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 1_000_000);
        let (client, admin, _invoice, _pool) = setup(&env);
        let new_admin = Address::generate(&env);
        client.propose_admin_change(&admin, &new_admin);
        client.cancel_admin_change(&admin);
        let result = client.try_finalize_admin_change(&admin);
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::NoAdminChangeProposed.into()
        );
    }

    #[test]
    fn test_cs_finalize_without_proposal_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _invoice, _pool) = setup(&env);
        let result = client.try_finalize_admin_change(&admin);
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::NoAdminChangeProposed.into()
        );
    }

    #[test]
    fn test_cs_cancel_without_proposal_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _invoice, _pool) = setup(&env);
        let result = client.try_cancel_admin_change(&admin);
        assert_eq!(
            result.unwrap_err().unwrap(),
            CreditScoreError::NoAdminChangeProposed.into()
        );
    }
}
