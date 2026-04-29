#![no_std]

// === AUTHORIZED CALLERS ===
// - Admin: initialize(), admin-only setters
// - Pool contract: record_payment(), record_default() (pool address stored in config)
// - Anyone: view functions (get_credit_score)

use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, BytesN, Env, String, Symbol, Vec,
};

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
const PTS_NEW_INVOICE: u32 = 5;

const LATE_PAYMENT_THRESHOLD_SECS: u64 = 7 * 24 * 60 * 60;
const UPGRADE_TIMELOCK_SECS: u64 = 86400; // 24 hours

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
pub enum DataKey {
    CreditScore(Address),
    PaymentHistory(Address),
    PaymentRecordIdx(Address, u32),
    InvoiceProcessed(u64),
    Admin,
    InvoiceContract,
    PoolContract,
    Initialized,
    ScoreVersion,
    Paused,
    ProposedWasmHash,
    UpgradeScheduledAt,
    /// Semantic version stored during initialize() (#237).
    ContractVersion,
}

const EVT: Symbol = symbol_short!("CREDIT");

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

#[contract]
pub struct CreditScoreContract;

fn calculate_score(
    total_invoices: u32,
    paid_on_time: u32,
    paid_late: u32,
    defaulted: u32,
    total_volume: i128,
    average_payment_days: i64,
) -> u32 {
    if total_invoices == 0 {
        return MIN_SCORE;
    }

    let mut score: i64 = BASE_SCORE as i64;

    score += (paid_on_time as i32 * PTS_PAID_ON_TIME as i32) as i64;
    score += (paid_late as i32 * PTS_PAID_LATE as i32) as i64;
    score += (defaulted as i32 * PTS_DEFAULTED) as i64;

    if total_invoices >= 5 {
        score += PTS_NEW_INVOICE as i64;
    }
    if total_invoices >= 10 {
        score += PTS_NEW_INVOICE as i64;
    }
    if total_invoices >= 20 {
        score += PTS_NEW_INVOICE as i64;
    }

    if average_payment_days < 0 {
        score += 20;
    } else if average_payment_days < 3 {
        score += 15;
    } else if average_payment_days < 7 {
        score += 10;
    } else if average_payment_days > 30 {
        score -= 15;
    }

    if total_volume > 100_000_000_000 {
        score += 25;
    } else if total_volume > 10_000_000_000 {
        score += 15;
    } else if total_volume > 1_000_000_000 {
        score += 5;
    }

    if score < MIN_SCORE as i64 {
        MIN_SCORE
    } else if score > MAX_SCORE as i64 {
        MAX_SCORE
    } else {
        score as u32
    }
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
            panic!("already initialized");
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
        // Store compile-time version (#237)
        env.storage()
            .instance()
            .set(&DataKey::ContractVersion, &parse_credit_score_version());
    }

    /// Returns the semantic version of this deployed credit-score contract (#237).
    pub fn version(env: Env) -> CreditScoreVersion {
        env.storage()
            .instance()
            .get(&DataKey::ContractVersion)
            .unwrap_or_else(parse_credit_score_version)
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

    pub fn record_payment(
        env: Env,
        caller: Address,
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

        if caller != pool {
            pool.require_auth();
        }

        require_not_paused(&env);

        if env
            .storage()
            .persistent()
            .has(&DataKey::InvoiceProcessed(invoice_id))
        {
            panic!("invoice already processed");
        }

        let status = if paid_at <= due_date {
            PaymentStatus::PaidOnTime
        } else if paid_at <= due_date + LATE_PAYMENT_THRESHOLD_SECS {
            PaymentStatus::PaidLate
        } else {
            PaymentStatus::Defaulted
        };

        let days_late: i64 = if paid_at > due_date {
            (paid_at - due_date) as i64 / (24 * 60 * 60)
        } else {
            -((due_date - paid_at) as i64 / (24 * 60 * 60))
        };

        let record = PaymentRecord {
            invoice_id,
            sme: sme.clone(),
            amount,
            due_date,
            paid_at,
            status: status.clone(),
            days_late,
        };

        let mut credit_data = Self::get_or_create_credit_data(&env, &sme);

        let history_len: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentHistory(sme.clone()))
            .unwrap_or(0);

        env.storage().persistent().set(
            &DataKey::PaymentRecordIdx(sme.clone(), history_len),
            &record,
        );
        env.storage()
            .instance()
            .set(&DataKey::PaymentHistory(sme.clone()), &(history_len + 1));

        // Capture the previous paid count before incrementing, for the running average.
        let prev_paid = (credit_data.paid_on_time + credit_data.paid_late) as i64;

        match status {
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
        credit_data.total_volume += amount;
        // Only paid (on-time + late) invoices contribute to the average; defaults are excluded.
        // Running sum = previous_average * previous_paid_count + new_days_late
        credit_data.average_payment_days = calculate_average_payment_days(
            credit_data.paid_on_time,
            credit_data.paid_late,
            credit_data.average_payment_days * prev_paid + days_late,
        );
        credit_data.score = calculate_score(
            credit_data.total_invoices,
            credit_data.paid_on_time,
            credit_data.paid_late,
            credit_data.defaulted,
            credit_data.total_volume,
            credit_data.average_payment_days,
        );
        credit_data.last_updated = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::CreditScore(sme.clone()), &credit_data);
        env.storage()
            .persistent()
            .set(&DataKey::InvoiceProcessed(invoice_id), &true);

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

    pub fn record_default(
        env: Env,
        caller: Address,
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

        if caller != pool {
            pool.require_auth();
        }

        require_not_paused(&env);

        if env
            .storage()
            .persistent()
            .has(&DataKey::InvoiceProcessed(invoice_id))
        {
            panic!("invoice already processed");
        }

        let defaulted_at = env.ledger().timestamp();
        let days_late = if defaulted_at > due_date {
            (defaulted_at - due_date) as i64 / (24 * 60 * 60)
        } else {
            0
        };

        let record = PaymentRecord {
            invoice_id,
            sme: sme.clone(),
            amount,
            due_date,
            paid_at: defaulted_at,
            status: PaymentStatus::Defaulted,
            days_late,
        };

        let mut credit_data = Self::get_or_create_credit_data(&env, &sme);

        let history_len: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentHistory(sme.clone()))
            .unwrap_or(0);

        env.storage().persistent().set(
            &DataKey::PaymentRecordIdx(sme.clone(), history_len),
            &record,
        );
        env.storage()
            .instance()
            .set(&DataKey::PaymentHistory(sme.clone()), &(history_len + 1));

        credit_data.defaulted += 1;
        credit_data.total_invoices += 1;
        credit_data.total_volume += amount;
        // Defaults do not affect average_payment_days — only paid invoices contribute.
        credit_data.score = calculate_score(
            credit_data.total_invoices,
            credit_data.paid_on_time,
            credit_data.paid_late,
            credit_data.defaulted,
            credit_data.total_volume,
            credit_data.average_payment_days,
        );
        credit_data.last_updated = env.ledger().timestamp();

        env.storage()
            .persistent()
            .set(&DataKey::CreditScore(sme.clone()), &credit_data);
        env.storage()
            .persistent()
            .set(&DataKey::InvoiceProcessed(invoice_id), &true);

        env.events().publish(
            (EVT, symbol_short!("default")),
            (sme, invoice_id, credit_data.score, env.ledger().timestamp()),
        );
    }

    pub fn get_credit_score(env: Env, sme: Address) -> CreditScoreData {
        Self::get_or_create_credit_data(&env, &sme)
    }

    pub fn get_payment_history(env: Env, sme: Address) -> Vec<PaymentRecord> {
        let history_len: u32 = env
            .storage()
            .instance()
            .get(&DataKey::PaymentHistory(sme.clone()))
            .unwrap_or(0);

        let mut records = Vec::new(&env);
        for i in 0..history_len {
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

    pub fn get_payment_record(env: Env, sme: Address, index: u32) -> Option<PaymentRecord> {
        env.storage()
            .persistent()
            .get(&DataKey::PaymentRecordIdx(sme, index))
    }

    pub fn get_payment_history_length(env: Env, sme: Address) -> u32 {
        env.storage()
            .instance()
            .get(&DataKey::PaymentHistory(sme))
            .unwrap_or(0)
    }

    pub fn get_score_band(env: Env, score: u32) -> String {
        if score >= 800 {
            String::from_str(&env, "Excellent")
        } else if score >= 740 {
            String::from_str(&env, "Very Good")
        } else if score >= 670 {
            String::from_str(&env, "Good")
        } else if score >= 580 {
            String::from_str(&env, "Fair")
        } else if score >= 500 {
            String::from_str(&env, "Poor")
        } else {
            String::from_str(&env, "Very Poor")
        }
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

    fn get_or_create_credit_data(env: &Env, sme: &Address) -> CreditScoreData {
        if let Some(data) = env
            .storage()
            .persistent()
            .get(&DataKey::CreditScore(sme.clone()))
        {
            data
        } else {
            CreditScoreData {
                sme: sme.clone(),
                score: MIN_SCORE,
                total_invoices: 0,
                paid_on_time: 0,
                paid_late: 0,
                defaulted: 0,
                total_volume: 0,
                average_payment_days: 0,
                last_updated: env.ledger().timestamp(),
                score_version: 1,
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
            panic!("unauthorized");
        }
    }

    pub fn propose_upgrade(env: Env, admin: Address, wasm_hash: BytesN<32>) {
        admin.require_auth();
        Self::require_admin(&env, &admin);
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
        Self::require_admin(&env, &admin);
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
}

#[cfg(test)]
mod test {
    use super::*;
    use soroban_sdk::{testutils::Address as _, testutils::Ledger, Env};

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
    #[should_panic(expected = "invoice already processed")]
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

        client.record_payment(
            &pool,
            &1,
            &sme,
            &1_000_000_000i128,
            &due_date,
            &(due_date - 1000),
        );
    }

    #[test]
    fn test_score_bands() {
        let env = Env::default();
        env.mock_all_auths();

        let (client, _admin, _invoice, _pool) = setup(&env);

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
        let mut seed: u64 = 0xDEAD_BEEF_1234_5678;
        let lcg = |s: &mut u64| -> u64 {
            *s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *s
        };

        for _ in 0..100 {
            let total_invoices = (lcg(&mut seed) % 50 + 1) as u32;
            let paid_on_time = (lcg(&mut seed) % (total_invoices as u64 + 1)) as u32;
            let remaining = total_invoices - paid_on_time;
            let paid_late = (lcg(&mut seed) % (remaining as u64 + 1)) as u32;
            let defaulted = remaining - paid_late;
            let total_volume = (lcg(&mut seed) % 200_000_000_000) as i128;
            let avg_days = (lcg(&mut seed) % 60) as i64 - 10; // -10 to +49

            let score = calculate_score(
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
    }

    // **Feature: credit-scoring, Property 2: Scoring formula monotonicity**
    // **Validates: Requirements 1.2, 1.3, 1.4**
    #[test]
    fn test_prop_scoring_formula_monotonicity() {
        // For any fixed base, adding an on-time payment scores >= adding a late payment
        // which scores >= adding a default.
        let mut seed: u64 = 0xCAFE_BABE_0000_0001;
        let lcg = |s: &mut u64| -> u64 {
            *s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *s
        };

        for _ in 0..100 {
            let base_invoices = (lcg(&mut seed) % 19 + 1) as u32;
            let base_on_time = (lcg(&mut seed) % (base_invoices as u64 + 1)) as u32;
            let base_remaining = base_invoices - base_on_time;
            let base_late = (lcg(&mut seed) % (base_remaining as u64 + 1)) as u32;
            let base_defaulted = base_remaining - base_late;
            let vol = (lcg(&mut seed) % 50_000_000_000) as i128;
            let avg = (lcg(&mut seed) % 20) as i64;

            let score_on_time = calculate_score(
                base_invoices + 1,
                base_on_time + 1,
                base_late,
                base_defaulted,
                vol,
                avg,
            );
            let score_late = calculate_score(
                base_invoices + 1,
                base_on_time,
                base_late + 1,
                base_defaulted,
                vol,
                avg,
            );
            let score_default = calculate_score(
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
    }

    // **Feature: credit-scoring, Property 3: Defaults dominate — score below BASE when defaults exceed on-time**
    // **Validates: Requirements 4.1**
    #[test]
    fn test_prop_defaults_dominate() {
        // When defaulted > paid_on_time and paid_late == 0, score must be < BASE_SCORE.
        let mut seed: u64 = 0xF00D_CAFE_ABCD_EF01;
        let lcg = |s: &mut u64| -> u64 {
            *s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            *s
        };

        for _ in 0..100 {
            let on_time = (lcg(&mut seed) % 10) as u32;
            let defaulted = on_time + (lcg(&mut seed) % 10 + 1) as u32; // always > on_time
            let total = on_time + defaulted;
            let vol = (lcg(&mut seed) % 5_000_000_000) as i128;
            let avg = (lcg(&mut seed) % 15) as i64;

            let score = calculate_score(total, on_time, 0, defaulted, vol, avg);
            assert!(
                score < BASE_SCORE,
                "score {} >= BASE_SCORE {} when defaulted({}) > on_time({}) with no late payments",
                score,
                BASE_SCORE,
                defaulted,
                on_time
            );
        }
    }

    // **Feature: credit-scoring, Property 7: Score band coverage**
    // **Validates: Requirements 6.1, 6.2, 6.3, 6.4, 6.5, 6.6**
    #[test]
    fn test_prop_score_band_coverage() {
        // For every score in [MIN_SCORE, MAX_SCORE], get_score_band returns the correct band.
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _invoice, _pool) = setup(&env);

        for score in MIN_SCORE..=MAX_SCORE {
            let band = client.get_score_band(&score);
            let expected = if score >= 800 {
                "Excellent"
            } else if score >= 740 {
                "Very Good"
            } else if score >= 670 {
                "Good"
            } else if score >= 580 {
                "Fair"
            } else if score >= 500 {
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
        // For any sequence of N records, get_payment_history returns N records in insertion order,
        // and get_payment_record(i) matches get_payment_history()[i].
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
            let n = (lcg(&mut seed) % 10 + 1) as u64;
            let due_date = 200_000u64;
            let mut expected_ids: soroban_sdk::Vec<u64> = soroban_sdk::Vec::new(&env);

            for i in 0..n {
                let invoice_id = trial * 20 + i + 1;
                expected_ids.push_back(invoice_id);
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

            // History length matches
            assert_eq!(
                client.get_payment_history_length(&sme),
                n as u32,
                "trial {}: history length mismatch",
                trial
            );

            // Full history in order
            let history = client.get_payment_history(&sme);
            assert_eq!(
                history.len(),
                n as u32,
                "trial {}: history vec length mismatch",
                trial
            );

            // Individual record lookup matches history
            for i in 0..n as u32 {
                let by_index = client.get_payment_record(&sme, &i).unwrap();
                let from_history = history.get(i).unwrap();
                assert_eq!(
                    by_index.invoice_id, from_history.invoice_id,
                    "trial {}: record {} invoice_id mismatch",
                    trial, i
                );
                assert_eq!(
                    by_index.invoice_id,
                    expected_ids.get(i).unwrap(),
                    "trial {}: record {} not in insertion order",
                    trial,
                    i
                );
            }
        }
    }

    // **Feature: credit-scoring, Property 9: Idempotency guard**
    // **Validates: Requirements 4.3**
    // Three separate should_panic tests cover all duplicate-processing paths.
    #[test]
    #[should_panic(expected = "invoice already processed")]
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
    #[should_panic(expected = "invoice already processed")]
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
    #[should_panic(expected = "invoice already processed")]
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
    #[should_panic(expected = "unauthorized")]
    fn test_credit_pause_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, _admin, _inv, _pool) = setup(&env);
        let intruder = Address::generate(&env);
        client.pause(&intruder);
    }

    #[test]
    #[should_panic(expected = "unauthorized")]
    fn test_credit_unpause_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (client, admin, _inv, _pool) = setup(&env);
        client.pause(&admin);
        let intruder = Address::generate(&env);
        client.unpause(&intruder);
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_record_payment_while_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 100_000);
        let (client, admin, _inv, pool) = setup(&env);
        let sme = Address::generate(&env);
        client.pause(&admin);
        client.record_payment(&pool, &1, &sme, &1_000i128, &200_000u64, &150_000u64);
    }

    #[test]
    #[should_panic(expected = "contract is paused")]
    fn test_record_default_while_paused_panics() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().with_mut(|l| l.timestamp = 200_000);
        let (client, admin, _inv, pool) = setup(&env);
        let sme = Address::generate(&env);
        client.pause(&admin);
        client.record_default(&pool, &1, &sme, &1_000i128, &100_000u64);
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
}
