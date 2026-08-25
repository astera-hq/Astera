#![no_std]

// === AUTHORIZED CALLERS ===
// - Admin: initialize(), register_screener()/deregister_screener(),
//   set_rescreening_interval(), set_screener_timelock(),
//   confirm_screener_registration(), pause()/unpause()
// - Registered screeners: submit_screening_result()
// - Anyone: request_review(), all read-only view functions (is_cleared, etc.)
//
// #867: On-chain compliance / sanctions screening registry. A single address
// registry (investors, SMEs) with time-bounded clearance, risk tiers, an
// append-only screening history, and a permissionless request_review path so
// the off-chain monitoring service can pause privileged actions without
// holding the screener role.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, symbol_short, Address,
    Env, String, Symbol, Vec,
};

const LEDGERS_PER_DAY: u32 = 17_280;
const REGISTRY_TTL: u32 = LEDGERS_PER_DAY * 365;
/// Default clearance lifetime before status lazily reverts to Unscreened.
const DEFAULT_RESCREENING_INTERVAL_SECS: u64 = 180 * 24 * 60 * 60; // 180 days
/// Default delay before a newly proposed screener becomes active.
const DEFAULT_SCREENER_TIMELOCK_SECS: u64 = 24 * 60 * 60; // 24 hours
const MAX_HISTORY_ENTRIES: u32 = 64;
const MAX_FLAGGED_LIST: u32 = 256;
const MAX_PENDING_LIST: u32 = 256;
const MAX_SCREENER_LIST: u32 = 64;
/// #927: minimum seconds between request_review calls for the same address
/// from the same caller. Prevents unbounded growth of the pending-review list.
const REVIEW_REQUEST_COOLDOWN_SECS: u64 = 24 * 60 * 60; // 24 hours

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ComplianceError {
    AlreadyInitialized = 0,
    NotInitialized = 1,
    Unauthorized = 2,
    ContractPaused = 3,
    InvalidConfig = 4,
    ScreenerAlreadyRegistered = 5,
    ScreenerNotRegistered = 6,
    ScreenerTimelockActive = 7,
    NoPendingScreener = 8,
    InvalidStatus = 9,
    AddressNotFound = 10,
    ListFull = 11,
    InvalidReason = 12,
    // #927: caller has already requested review for this address within the cooldown window.
    ReviewRequestTooSoon = 13,
    // Screener has already screened this subject within the rescreening interval.
    RescreeningTooSoon = 14,
    // #1042: a `*_via_ac` entrypoint was called but no `access_control`
    // contract has been configured via `set_access_control` yet.
    AccessControlNotConfigured = 15,
    // #1038: governance contract not configured
    GovernanceNotConfigured = 16,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ComplianceStatus {
    Unscreened,
    Cleared,
    Flagged,
    Blocked,
    PendingReview,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RiskTier {
    Low,
    Medium,
    High,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ComplianceRecord {
    pub address: Address,
    pub status: ComplianceStatus,
    pub reason_code: u32,
    pub risk_tier: RiskTier,
    pub screened_at: u64,
    pub screened_by: Address,
    pub expires_at: u64,
    pub notes_hash: String,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ScreeningHistoryEntry {
    pub status: ComplianceStatus,
    pub reason_code: u32,
    pub risk_tier: RiskTier,
    pub screened_at: u64,
    pub screened_by: Address,
    pub expires_at: u64,
    pub notes_hash: String,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct ScreenerSubmissionEntry {
    pub subject_address: Address,
    pub status: ComplianceStatus,
    pub reason_code: u32,
    pub risk_tier: RiskTier,
    pub screened_at: u64,
    pub expires_at: u64,
    pub notes_hash: String,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingScreener {
    pub address: Address,
    pub proposed_at: u64,
    pub effective_at: u64,
}

#[contracttype]
pub enum DataKey {
    Admin,
    Initialized,
    Paused,
    RescreeningInterval,
    ScreenerTimelockSecs,
    Record(Address),
    History(Address),
    Screener(Address),
    ScreenerIds,
    PendingScreener(Address),
    FlaggedIds,
    PendingReviewIds,
    // #927: last request_review timestamp keyed by (caller, target address).
    ReviewRequestedAt(Address, Address),
    // Screener submission history keyed by screener address.
    ScreenerSubmissions(Address),
    // Last screening timestamp keyed by (screener, subject address).
    LastScreenedAt(Address, Address),
    // #1042: multisig trust anchor. Additive — untouched, this stays unset
    // and every admin-gated entrypoint above works exactly as before.
    AccessControl,
    // #1038: governance contract address for governance-gated parameter changes
    Governance,
}

const EVT: Symbol = symbol_short!("COMPLY");

fn require_not_paused(env: &Env) {
    if env
        .storage()
        .instance()
        .get::<DataKey, bool>(&DataKey::Paused)
        .unwrap_or(false)
    {
        panic_with_error!(env, ComplianceError::ContractPaused);
    }
}

#[contract]
pub struct ComplianceContract;

#[contractimpl]
impl ComplianceContract {
    pub fn initialize(env: Env, admin: Address) {
        if env.storage().instance().has(&DataKey::Initialized) {
            panic_with_error!(&env, ComplianceError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Initialized, &true);
        env.storage().instance().set(&DataKey::Paused, &false);
        env.storage().instance().set(
            &DataKey::RescreeningInterval,
            &DEFAULT_RESCREENING_INTERVAL_SECS,
        );
        env.storage().instance().set(
            &DataKey::ScreenerTimelockSecs,
            &DEFAULT_SCREENER_TIMELOCK_SECS,
        );
        env.storage()
            .instance()
            .set(&DataKey::ScreenerIds, &Vec::<Address>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::FlaggedIds, &Vec::<Address>::new(&env));
        env.storage()
            .instance()
            .set(&DataKey::PendingReviewIds, &Vec::<Address>::new(&env));
        env.storage()
            .instance()
            .extend_ttl(REGISTRY_TTL, REGISTRY_TTL);
    }

    pub fn pause(env: Env, admin: Address) -> Result<(), ComplianceError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::Paused, &true);
        env.events().publish((EVT, symbol_short!("paused")), admin);
        Ok(())
    }

    pub fn unpause(env: Env, admin: Address) -> Result<(), ComplianceError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        env.storage().instance().set(&DataKey::Paused, &false);
        env.events()
            .publish((EVT, symbol_short!("unpaused")), admin);
        Ok(())
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Propose a screener. Becomes active after the configured timelock via
    /// `confirm_screener_registration` (or immediately when timelock is 0).
    pub fn register_screener(
        env: Env,
        admin: Address,
        screener: Address,
    ) -> Result<(), ComplianceError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        Self::register_screener_internal(&env, screener, admin)
    }

    fn register_screener_internal(
        env: &Env,
        screener: Address,
        caller: Address,
    ) -> Result<(), ComplianceError> {
        require_not_paused(env);

        if env
            .storage()
            .persistent()
            .has(&DataKey::Screener(screener.clone()))
        {
            return Err(ComplianceError::ScreenerAlreadyRegistered);
        }
        // #926: reject re-register while a pending registration already exists.
        // Otherwise lowering set_screener_timelock and re-calling register could
        // overwrite the frozen effective_at (or instantly activate with timelock 0).
        if env
            .storage()
            .instance()
            .has(&DataKey::PendingScreener(screener.clone()))
        {
            return Err(ComplianceError::ScreenerAlreadyRegistered);
        }

        let timelock: u64 = env
            .storage()
            .instance()
            .get(&DataKey::ScreenerTimelockSecs)
            .unwrap_or(DEFAULT_SCREENER_TIMELOCK_SECS);
        let now = env.ledger().timestamp();

        if timelock == 0 {
            Self::activate_screener(env, &screener)?;
        } else {
            // Freeze effective_at at registration time — confirm uses this value,
            // never the live ScreenerTimelockSecs config (#926).
            let pending = PendingScreener {
                address: screener.clone(),
                proposed_at: now,
                effective_at: now.saturating_add(timelock),
            };
            env.storage()
                .instance()
                .set(&DataKey::PendingScreener(screener.clone()), &pending);
            env.events().publish(
                (EVT, symbol_short!("scr_prop")),
                (caller, screener, pending.effective_at),
            );
        }
        Ok(())
    }

    /// Finalize a proposed screener once its timelock has elapsed.
    pub fn confirm_screener_registration(
        env: Env,
        admin: Address,
        screener: Address,
    ) -> Result<(), ComplianceError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        Self::confirm_screener_registration_internal(&env, screener)
    }

    fn confirm_screener_registration_internal(
        env: &Env,
        screener: Address,
    ) -> Result<(), ComplianceError> {
        require_not_paused(env);

        let pending: PendingScreener = env
            .storage()
            .instance()
            .get(&DataKey::PendingScreener(screener.clone()))
            .ok_or(ComplianceError::NoPendingScreener)?;

        let now = env.ledger().timestamp();
        if now < pending.effective_at {
            return Err(ComplianceError::ScreenerTimelockActive);
        }

        env.storage()
            .instance()
            .remove(&DataKey::PendingScreener(screener.clone()));
        Self::activate_screener(env, &screener)?;
        Ok(())
    }

    pub fn deregister_screener(
        env: Env,
        admin: Address,
        screener: Address,
    ) -> Result<(), ComplianceError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        Self::deregister_screener_internal(&env, screener, admin)
    }

    fn deregister_screener_internal(
        env: &Env,
        screener: Address,
        caller: Address,
    ) -> Result<(), ComplianceError> {
        require_not_paused(env);

        if !env
            .storage()
            .persistent()
            .has(&DataKey::Screener(screener.clone()))
        {
            // Also clear a pending proposal if present.
            if env
                .storage()
                .instance()
                .has(&DataKey::PendingScreener(screener.clone()))
            {
                env.storage()
                    .instance()
                    .remove(&DataKey::PendingScreener(screener.clone()));
                env.events()
                    .publish((EVT, symbol_short!("scr_can")), (caller, screener));
                return Ok(());
            }
            return Err(ComplianceError::ScreenerNotRegistered);
        }

        env.storage()
            .persistent()
            .remove(&DataKey::Screener(screener.clone()));
        Self::remove_from_list(env, &DataKey::ScreenerIds, &screener);
        env.events()
            .publish((EVT, symbol_short!("scr_del")), (caller, screener));
        Ok(())
    }

    pub fn is_screener(env: Env, address: Address) -> bool {
        env.storage().persistent().has(&DataKey::Screener(address))
    }

    pub fn list_screeners(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::ScreenerIds)
            .unwrap_or(Vec::new(&env))
    }

    pub fn set_rescreening_interval(
        env: Env,
        admin: Address,
        secs: u64,
    ) -> Result<(), ComplianceError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        if secs == 0 {
            return Err(ComplianceError::InvalidConfig);
        }
        env.storage()
            .instance()
            .set(&DataKey::RescreeningInterval, &secs);
        env.events()
            .publish((EVT, symbol_short!("int_set")), (admin, secs));
        Ok(())
    }

    pub fn get_rescreening_interval(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::RescreeningInterval)
            .unwrap_or(DEFAULT_RESCREENING_INTERVAL_SECS)
    }

    /// Updates the timelock applied to *future* `register_screener` calls only.
    /// #926: must not rewrite `effective_at` on any in-flight PendingScreener —
    /// confirm always uses the value frozen at registration time.
    pub fn set_screener_timelock(
        env: Env,
        admin: Address,
        secs: u64,
    ) -> Result<(), ComplianceError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&DataKey::ScreenerTimelockSecs, &secs);
        env.events()
            .publish((EVT, symbol_short!("tl_set")), (admin, secs));
        Ok(())
    }

    pub fn get_screener_timelock(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::ScreenerTimelockSecs)
            .unwrap_or(DEFAULT_SCREENER_TIMELOCK_SECS)
    }

    /// Submit a screening decision for an address. Auth-gated to registered
    /// screeners (admin is also treated as a screener).
    #[allow(clippy::too_many_arguments)]
    pub fn submit_screening_result(
        env: Env,
        screener: Address,
        address: Address,
        status: ComplianceStatus,
        reason_code: u32,
        risk_tier: RiskTier,
        expires_at: u64,
        notes_hash: String,
    ) -> Result<(), ComplianceError> {
        screener.require_auth();
        require_not_paused(&env);
        Self::require_screener_or_admin(&env, &screener)?;

        match status {
            ComplianceStatus::Unscreened => return Err(ComplianceError::InvalidStatus),
            ComplianceStatus::Cleared
            | ComplianceStatus::Flagged
            | ComplianceStatus::Blocked
            | ComplianceStatus::PendingReview => {}
        }

        let now = env.ledger().timestamp();

        // Enforce rescreening interval: screener cannot rescreen same subject within interval
        let last_screened_key = DataKey::LastScreenedAt(screener.clone(), address.clone());
        if let Some(last_at) = env
            .storage()
            .instance()
            .get::<DataKey, u64>(&last_screened_key)
        {
            let interval = Self::get_rescreening_interval(env.clone());
            if now < last_at.saturating_add(interval) {
                return Err(ComplianceError::RescreeningTooSoon);
            }
        }
        let effective_expires = if expires_at == 0 {
            let interval = Self::get_rescreening_interval(env.clone());
            now.saturating_add(interval)
        } else if expires_at <= now && matches!(status, ComplianceStatus::Cleared) {
            return Err(ComplianceError::InvalidConfig);
        } else {
            expires_at
        };

        let record = ComplianceRecord {
            address: address.clone(),
            status: status.clone(),
            reason_code,
            risk_tier: risk_tier.clone(),
            screened_at: now,
            screened_by: screener.clone(),
            expires_at: effective_expires,
            notes_hash: notes_hash.clone(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Record(address.clone()), &record);
        env.storage().persistent().extend_ttl(
            &DataKey::Record(address.clone()),
            REGISTRY_TTL,
            REGISTRY_TTL,
        );

        Self::append_history(
            &env,
            &address,
            ScreeningHistoryEntry {
                status: status.clone(),
                reason_code,
                risk_tier: risk_tier.clone(),
                screened_at: now,
                screened_by: screener.clone(),
                expires_at: effective_expires,
                notes_hash: notes_hash.clone(),
            },
        );

        Self::append_screener_submission(
            &env,
            &screener,
            ScreenerSubmissionEntry {
                subject_address: address.clone(),
                status: status.clone(),
                reason_code,
                risk_tier,
                screened_at: now,
                expires_at: effective_expires,
                notes_hash,
            },
        );

        // Update last screening timestamp for this (screener, subject) pair
        env.storage().instance().set(&last_screened_key, &now);

        Self::sync_status_lists(&env, &address, &status);

        env.events().publish(
            (EVT, symbol_short!("screened")),
            (address, status, reason_code, screener, effective_expires),
        );
        Ok(())
    }

    /// Permissionless flagging: moves a Cleared address to PendingReview so
    /// privileged actions pause until a screener re-clears them. Callable by
    /// the off-chain monitoring service without screener privileges.
    /// #927: rate-limited per (caller, address) pair to prevent list spam.
    pub fn request_review(
        env: Env,
        caller: Address,
        address: Address,
        reason: String,
    ) -> Result<(), ComplianceError> {
        caller.require_auth();
        require_not_paused(&env);

        let now = env.ledger().timestamp();

        // #927: enforce cooldown — same caller cannot re-request review for the
        // same address within REVIEW_REQUEST_COOLDOWN_SECS.
        let cooldown_key = DataKey::ReviewRequestedAt(caller.clone(), address.clone());
        if let Some(last_at) = env.storage().instance().get::<DataKey, u64>(&cooldown_key) {
            if now < last_at.saturating_add(REVIEW_REQUEST_COOLDOWN_SECS) {
                return Err(ComplianceError::ReviewRequestTooSoon);
            }
        }
        env.storage().instance().set(&cooldown_key, &now);
        let existing: Option<ComplianceRecord> = env
            .storage()
            .persistent()
            .get(&DataKey::Record(address.clone()));

        // Only interrupt an actively-cleared (non-expired) record.
        let was_cleared = match &existing {
            Some(r) => {
                matches!(r.status, ComplianceStatus::Cleared)
                    && (r.expires_at == 0 || now < r.expires_at)
            }
            None => false,
        };

        let record = ComplianceRecord {
            address: address.clone(),
            status: ComplianceStatus::PendingReview,
            reason_code: 0,
            risk_tier: existing
                .as_ref()
                .map(|r| r.risk_tier.clone())
                .unwrap_or(RiskTier::Medium),
            screened_at: now,
            screened_by: caller.clone(),
            expires_at: 0,
            notes_hash: reason.clone(),
        };

        env.storage()
            .persistent()
            .set(&DataKey::Record(address.clone()), &record);
        env.storage().persistent().extend_ttl(
            &DataKey::Record(address.clone()),
            REGISTRY_TTL,
            REGISTRY_TTL,
        );

        Self::append_history(
            &env,
            &address,
            ScreeningHistoryEntry {
                status: ComplianceStatus::PendingReview,
                reason_code: 0,
                risk_tier: record.risk_tier.clone(),
                screened_at: now,
                screened_by: caller.clone(),
                expires_at: 0,
                notes_hash: reason.clone(),
            },
        );

        Self::append_screener_submission(
            &env,
            &caller,
            ScreenerSubmissionEntry {
                subject_address: address.clone(),
                status: ComplianceStatus::PendingReview,
                reason_code: 0,
                risk_tier: record.risk_tier.clone(),
                screened_at: now,
                expires_at: 0,
                notes_hash: reason,
            },
        );

        Self::sync_status_lists(&env, &address, &ComplianceStatus::PendingReview);

        env.events().publish(
            (EVT, symbol_short!("review")),
            (address, caller, was_cleared),
        );
        Ok(())
    }

    /// Hot-path read used by pool/invoice gates. Returns true only when the
    /// address has a non-expired Cleared record. Lazy expiry: a stale Cleared
    /// past `expires_at` returns false without requiring a write.
    pub fn is_cleared(env: Env, address: Address) -> bool {
        let record: Option<ComplianceRecord> =
            env.storage().persistent().get(&DataKey::Record(address));
        match record {
            Some(r) => {
                if !matches!(r.status, ComplianceStatus::Cleared) {
                    return false;
                }
                let now = env.ledger().timestamp();
                r.expires_at == 0 || now < r.expires_at
            }
            None => false,
        }
    }

    /// Effective status with lazy expiry applied (Cleared past expires_at → Unscreened).
    pub fn get_effective_status(env: Env, address: Address) -> ComplianceStatus {
        let record: Option<ComplianceRecord> =
            env.storage().persistent().get(&DataKey::Record(address));
        match record {
            Some(r) => {
                if matches!(r.status, ComplianceStatus::Cleared) {
                    let now = env.ledger().timestamp();
                    if r.expires_at != 0 && now >= r.expires_at {
                        return ComplianceStatus::Unscreened;
                    }
                }
                r.status
            }
            None => ComplianceStatus::Unscreened,
        }
    }

    /// #928: Batch variant of get_effective_status. Returns statuses in the
    /// same order as the input addresses, reducing cross-contract calls for
    /// flows that touch multiple addresses (e.g. fund_multiple_invoices).
    pub fn get_effective_status_batch(env: Env, addresses: Vec<Address>) -> Vec<ComplianceStatus> {
        let now = env.ledger().timestamp();
        let mut results = Vec::new(&env);
        for i in 0..addresses.len() {
            let addr = addresses.get(i).unwrap();
            let status = match env
                .storage()
                .persistent()
                .get::<DataKey, ComplianceRecord>(&DataKey::Record(addr))
            {
                Some(r) => {
                    if matches!(r.status, ComplianceStatus::Cleared)
                        && r.expires_at != 0
                        && now >= r.expires_at
                    {
                        ComplianceStatus::Unscreened
                    } else {
                        r.status
                    }
                }
                None => ComplianceStatus::Unscreened,
            };
            results.push_back(status);
        }
        results
    }

    pub fn get_compliance_record(env: Env, address: Address) -> Option<ComplianceRecord> {
        env.storage().persistent().get(&DataKey::Record(address))
    }

    pub fn get_screening_history(env: Env, address: Address) -> Vec<ScreeningHistoryEntry> {
        env.storage()
            .persistent()
            .get(&DataKey::History(address))
            .unwrap_or(Vec::new(&env))
    }

    /// Returns all screening submissions made by a specific screener across all subjects.
    /// This allows screeners to audit their own submission history.
    pub fn get_screener_submissions(env: Env, screener: Address) -> Vec<ScreenerSubmissionEntry> {
        env.storage()
            .persistent()
            .get(&DataKey::ScreenerSubmissions(screener))
            .unwrap_or(Vec::new(&env))
    }

    pub fn list_flagged(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::FlaggedIds)
            .unwrap_or(Vec::new(&env))
    }

    pub fn list_pending_review(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&DataKey::PendingReviewIds)
            .unwrap_or(Vec::new(&env))
    }

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .unwrap_or_else(|| panic_with_error!(&env, ComplianceError::NotInitialized))
    }

    // #1042: multisig admin path, additive to the legacy single-admin
    // functions above — see access_control/src/lib.rs for the full
    // propose/approve/execute lifecycle. `set_access_control` bootstraps
    // the trust anchor (still gated by the legacy admin key); every
    // `*_via_ac` entrypoint below then trusts only calls that carry the
    // configured `access_control` contract's own on-chain identity.

    pub fn set_access_control(
        env: Env,
        admin: Address,
        access_control: Address,
    ) -> Result<(), ComplianceError> {
        admin.require_auth();
        Self::require_admin(&env, &admin)?;
        env.storage()
            .instance()
            .set(&DataKey::AccessControl, &access_control);
        env.events()
            .publish((EVT, symbol_short!("set_ac")), (admin, access_control));
        Ok(())
    }

    pub fn get_access_control(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::AccessControl)
    }

    // #1038: Bootstrap the governance contract address. Admin-gated one-time setup.
    pub fn set_governance_address(
        env: Env,
        admin: Address,
        governance: Address,
    ) -> Result<(), ComplianceError> {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ComplianceError::NotInitialized)?;
        if admin != stored_admin {
            return Err(ComplianceError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&DataKey::Governance, &governance);
        env.events()
            .publish((EVT, symbol_short!("set_gov")), (admin, governance));
        Ok(())
    }

    // #1038: Get the configured governance contract address.
    pub fn get_governance_address(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::Governance)
    }

    /// #1042: rotates the trust anchor itself through the currently
    /// configured `access_control` contract rather than the legacy admin
    /// key.
    pub fn set_access_control_via_ac(
        env: Env,
        access_control: Address,
        new_access_control: Address,
    ) -> Result<(), ComplianceError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        env.storage()
            .instance()
            .set(&DataKey::AccessControl, &new_access_control);
        env.events().publish(
            (EVT, symbol_short!("ac_rot")),
            (access_control, new_access_control),
        );
        Ok(())
    }

    pub fn set_paused_via_ac(
        env: Env,
        access_control: Address,
        paused: bool,
    ) -> Result<(), ComplianceError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        env.storage().instance().set(&DataKey::Paused, &paused);
        env.events()
            .publish((EVT, symbol_short!("ac_pause")), (access_control, paused));
        Ok(())
    }

    // ---- #1038: Governance-gated parameter changes ----

    // #1038: Set rescreening interval via governance proposal.
    pub fn set_rescreening_interval_via_governance(
        env: Env,
        governance: Address,
        secs: u64,
    ) -> Result<(), ComplianceError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        if secs < 86_400 {
            return Err(ComplianceError::InvalidConfig);
        }
        env.storage()
            .instance()
            .set(&DataKey::RescreeningInterval, &secs);
        env.events()
            .publish((EVT, symbol_short!("gov_rescreen")), (governance, secs));
        Ok(())
    }

    // #1038: Set screener timelock via governance proposal.
    pub fn set_screener_timelock_via_governance(
        env: Env,
        governance: Address,
        secs: u64,
    ) -> Result<(), ComplianceError> {
        governance.require_auth();
        Self::require_governance(&env, &governance)?;
        if secs < 3_600 {
            return Err(ComplianceError::InvalidConfig);
        }
        env.storage()
            .instance()
            .set(&DataKey::ScreenerTimelockSecs, &secs);
        env.events()
            .publish((EVT, symbol_short!("gov_timelock")), (governance, secs));
        Ok(())
    }

    pub fn register_screener_via_ac(
        env: Env,
        access_control: Address,
        screener: Address,
    ) -> Result<(), ComplianceError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        Self::register_screener_internal(&env, screener, access_control)
    }

    pub fn confirm_screener_via_ac(
        env: Env,
        access_control: Address,
        screener: Address,
    ) -> Result<(), ComplianceError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        Self::confirm_screener_registration_internal(&env, screener)
    }

    pub fn deregister_screener_via_ac(
        env: Env,
        access_control: Address,
        screener: Address,
    ) -> Result<(), ComplianceError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        Self::deregister_screener_internal(&env, screener, access_control)
    }

    pub fn set_rescreening_interval_via_ac(
        env: Env,
        access_control: Address,
        secs: u64,
    ) -> Result<(), ComplianceError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        if secs == 0 {
            return Err(ComplianceError::InvalidConfig);
        }
        env.storage()
            .instance()
            .set(&DataKey::RescreeningInterval, &secs);
        env.events()
            .publish((EVT, symbol_short!("int_set")), (access_control, secs));
        Ok(())
    }

    pub fn set_screener_timelock_via_ac(
        env: Env,
        access_control: Address,
        secs: u64,
    ) -> Result<(), ComplianceError> {
        access_control.require_auth();
        Self::require_access_control(&env, &access_control)?;
        env.storage()
            .instance()
            .set(&DataKey::ScreenerTimelockSecs, &secs);
        env.events()
            .publish((EVT, symbol_short!("tl_set")), (access_control, secs));
        Ok(())
    }
}

impl ComplianceContract {
    fn require_admin(env: &Env, admin: &Address) -> Result<(), ComplianceError> {
        let stored: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ComplianceError::NotInitialized)?;
        if admin != &stored {
            return Err(ComplianceError::Unauthorized);
        }
        Ok(())
    }

    fn require_access_control(env: &Env, caller: &Address) -> Result<(), ComplianceError> {
        let configured: Address = env
            .storage()
            .instance()
            .get(&DataKey::AccessControl)
            .ok_or(ComplianceError::AccessControlNotConfigured)?;
        if caller != &configured {
            return Err(ComplianceError::Unauthorized);
        }
        Ok(())
    }

    // #1038: Helper function to verify the caller is the configured governance contract
    fn require_governance(env: &Env, caller: &Address) -> Result<(), ComplianceError> {
        let configured: Address = env
            .storage()
            .instance()
            .get(&DataKey::Governance)
            .ok_or(ComplianceError::GovernanceNotConfigured)?;
        if caller != &configured {
            return Err(ComplianceError::Unauthorized);
        }
        Ok(())
    }

    fn require_screener_or_admin(env: &Env, caller: &Address) -> Result<(), ComplianceError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(ComplianceError::NotInitialized)?;
        if caller == &admin {
            return Ok(());
        }
        if env
            .storage()
            .persistent()
            .has(&DataKey::Screener(caller.clone()))
        {
            return Ok(());
        }
        Err(ComplianceError::Unauthorized)
    }

    fn activate_screener(env: &Env, screener: &Address) -> Result<(), ComplianceError> {
        if env
            .storage()
            .persistent()
            .has(&DataKey::Screener(screener.clone()))
        {
            return Err(ComplianceError::ScreenerAlreadyRegistered);
        }
        let mut ids: Vec<Address> = env
            .storage()
            .instance()
            .get(&DataKey::ScreenerIds)
            .unwrap_or(Vec::new(env));
        if ids.len() >= MAX_SCREENER_LIST {
            return Err(ComplianceError::ListFull);
        }
        ids.push_back(screener.clone());
        env.storage().instance().set(&DataKey::ScreenerIds, &ids);
        env.storage()
            .persistent()
            .set(&DataKey::Screener(screener.clone()), &true);
        env.storage().persistent().extend_ttl(
            &DataKey::Screener(screener.clone()),
            REGISTRY_TTL,
            REGISTRY_TTL,
        );
        env.events()
            .publish((EVT, symbol_short!("scr_reg")), screener.clone());
        Ok(())
    }

    fn append_history(env: &Env, address: &Address, entry: ScreeningHistoryEntry) {
        let key = DataKey::History(address.clone());
        let mut history: Vec<ScreeningHistoryEntry> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));
        if history.len() >= MAX_HISTORY_ENTRIES {
            // Drop oldest to keep storage bounded.
            let mut trimmed = Vec::new(env);
            for i in 1..history.len() {
                if let Some(e) = history.get(i) {
                    trimmed.push_back(e);
                }
            }
            history = trimmed;
        }
        history.push_back(entry);
        env.storage().persistent().set(&key, &history);
        env.storage()
            .persistent()
            .extend_ttl(&key, REGISTRY_TTL, REGISTRY_TTL);
    }

    fn append_screener_submission(env: &Env, screener: &Address, entry: ScreenerSubmissionEntry) {
        let key = DataKey::ScreenerSubmissions(screener.clone());
        let mut submissions: Vec<ScreenerSubmissionEntry> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or(Vec::new(env));
        if submissions.len() >= MAX_HISTORY_ENTRIES {
            // Drop oldest to keep storage bounded.
            let mut trimmed = Vec::new(env);
            for i in 1..submissions.len() {
                if let Some(e) = submissions.get(i) {
                    trimmed.push_back(e);
                }
            }
            submissions = trimmed;
        }
        submissions.push_back(entry);
        env.storage().persistent().set(&key, &submissions);
        env.storage()
            .persistent()
            .extend_ttl(&key, REGISTRY_TTL, REGISTRY_TTL);
    }

    fn sync_status_lists(env: &Env, address: &Address, status: &ComplianceStatus) {
        match status {
            ComplianceStatus::Flagged | ComplianceStatus::Blocked => {
                Self::add_to_list(env, &DataKey::FlaggedIds, address, MAX_FLAGGED_LIST);
                Self::remove_from_list(env, &DataKey::PendingReviewIds, address);
            }
            ComplianceStatus::PendingReview => {
                Self::add_to_list(env, &DataKey::PendingReviewIds, address, MAX_PENDING_LIST);
                Self::remove_from_list(env, &DataKey::FlaggedIds, address);
            }
            ComplianceStatus::Cleared | ComplianceStatus::Unscreened => {
                Self::remove_from_list(env, &DataKey::FlaggedIds, address);
                Self::remove_from_list(env, &DataKey::PendingReviewIds, address);
            }
        }
    }

    fn add_to_list(env: &Env, key: &DataKey, address: &Address, max: u32) {
        let mut list: Vec<Address> = env.storage().instance().get(key).unwrap_or(Vec::new(env));
        for i in 0..list.len() {
            if let Some(a) = list.get(i) {
                if &a == address {
                    return;
                }
            }
        }
        if list.len() >= max {
            // Drop oldest entry to make room.
            let mut trimmed = Vec::new(env);
            for i in 1..list.len() {
                if let Some(a) = list.get(i) {
                    trimmed.push_back(a);
                }
            }
            list = trimmed;
        }
        list.push_back(address.clone());
        env.storage().instance().set(key, &list);
    }

    fn remove_from_list(env: &Env, key: &DataKey, address: &Address) {
        let list: Vec<Address> = env.storage().instance().get(key).unwrap_or(Vec::new(env));
        let mut next = Vec::new(env);
        for i in 0..list.len() {
            if let Some(a) = list.get(i) {
                if &a != address {
                    next.push_back(a);
                }
            }
        }
        env.storage().instance().set(key, &next);
    }
}

#[cfg(test)]
mod test {
    // Unit tests live in tests/*.rs integration crates.
}
