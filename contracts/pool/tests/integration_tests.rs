use pool::{FundingPool, FundingPoolClient, PoolError};
use soroban_sdk::{
    contract, contractimpl,
    testutils::{Address as _, Ledger},
    token, Address, Env, Symbol,
};

#[contract]
pub struct DummyShare;

#[contractimpl]
impl DummyShare {
    pub fn total_supply(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "tot"))
            .unwrap_or(0)
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        env.storage().persistent().get(&id).unwrap_or(0)
    }

    pub fn mint(env: Env, to: Address, amount: i128) {
        let total = Self::total_supply(env.clone());
        let balance = Self::balance(env.clone(), to.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "tot"), &(total + amount));
        env.storage().persistent().set(&to, &(balance + amount));
    }

    pub fn burn(env: Env, from: Address, amount: i128) {
        let total = Self::total_supply(env.clone());
        let balance = Self::balance(env.clone(), from.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "tot"), &(total - amount));
        env.storage().persistent().set(&from, &(balance - amount));
    }
}

#[contract]
pub struct DummyInvoice;

#[contractimpl]
impl DummyInvoice {
    pub fn get_authorized_pool(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "pool"))
            .expect("not initialized")
    }

    pub fn set_pool(env: Env, pool: Address) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "pool"), &pool);
    }
}

fn setup(env: &Env) -> (FundingPoolClient<'_>, Address, Address) {
    env.ledger().with_mut(|l| l.timestamp = 100_000);
    let contract_id = env.register(FundingPool, ());
    let client = FundingPoolClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin)
        .address();
    let invoice_contract = env.register(DummyInvoice, ());
    DummyInvoiceClient::new(env, &invoice_contract).set_pool(&contract_id);
    let share_token = env.register(DummyShare, ());

    client.initialize(&admin, &usdc_id, &share_token, &invoice_contract);
    client.set_max_investor_concentration(&admin, &10_000u32);
    (client, admin, usdc_id)
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    token::StellarAssetClient::new(env, token_id).mint(to, &amount);
}

#[test]
fn test_kyc_blocks_deposit_when_required() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let investor = Address::generate(&env);

    client.set_kyc_required(&admin, &true);
    mint(&env, &usdc_id, &investor, 1_000);

    let result = client.try_deposit(&investor, &usdc_id, &1_000);
    assert_eq!(result, Err(Ok(PoolError::KycNotRequested)));
}

#[test]
fn test_kyc_allows_deposit_after_approval() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let investor = Address::generate(&env);

    client.set_kyc_required(&admin, &true);
    client.set_investor_kyc(&admin, &investor, &true);
    mint(&env, &usdc_id, &investor, 1_500);

    client.deposit(&investor, &usdc_id, &1_500);

    let totals = client.get_token_totals(&usdc_id);
    assert_eq!(totals.pool_value, 1_500);
}

#[test]
fn test_kyc_not_required_by_default() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, usdc_id) = setup(&env);
    let investor = Address::generate(&env);

    assert!(!client.kyc_required());
    mint(&env, &usdc_id, &investor, 750);
    client.deposit(&investor, &usdc_id, &750);

    let totals = client.get_token_totals(&usdc_id);
    assert_eq!(totals.pool_value, 750);
}

#[test]
fn test_kyc_required_flag_toggle() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let investor = Address::generate(&env);

    mint(&env, &usdc_id, &investor, 3_000);

    client.set_kyc_required(&admin, &true);
    let blocked = client.try_deposit(&investor, &usdc_id, &1_000);
    assert_eq!(blocked, Err(Ok(PoolError::KycNotRequested)));

    client.set_kyc_required(&admin, &false);
    client.deposit(&investor, &usdc_id, &1_000);

    client.set_kyc_required(&admin, &true);
    let blocked_again = client.try_deposit(&investor, &usdc_id, &1_000);
    assert_eq!(blocked_again, Err(Ok(PoolError::KycNotRequested)));
}

#[test]
fn test_non_admin_cannot_approve_investor_kyc() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _usdc_id) = setup(&env);
    let attacker = Address::generate(&env);
    let investor = Address::generate(&env);

    let result = client.try_set_investor_kyc(&attacker, &investor, &true);
    assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
}

#[test]
fn test_non_admin_cannot_set_kyc_required() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _usdc_id) = setup(&env);
    let attacker = Address::generate(&env);

    let result = client.try_set_kyc_required(&attacker, &true);
    assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
}

#[test]
fn test_propose_set_collateral_config() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _usdc_id) = setup(&env);

    let new_threshold = 50_000_000_000i128;
    let new_collateral_bps = 3000u32;

    let proposal_id = client.propose_operation(
        &admin,
        &pool::AdminOperation::SetCollateralConfig(new_threshold, new_collateral_bps),
    );

    assert_eq!(proposal_id, 1);

    let proposal = client.get_proposal(&proposal_id);
    assert!(proposal.is_some());
    let prop = proposal.unwrap();
    assert!(!prop.executed);
    assert!(!prop.cancelled);
    assert_eq!(prop.proposer, admin);
}

#[test]
fn test_execute_set_collateral_config_after_delay() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _usdc_id) = setup(&env);

    let new_threshold = 50_000_000_000i128;
    let new_collateral_bps = 3000u32;

    let proposal_id = client.propose_operation(
        &admin,
        &pool::AdminOperation::SetCollateralConfig(new_threshold, new_collateral_bps),
    );

    env.ledger().with_mut(|l| l.timestamp += 86_400);

    client.execute_operation(&admin, &proposal_id);

    let config = client.get_collateral_config();
    assert_eq!(config.threshold, new_threshold);
    assert_eq!(config.collateral_bps, new_collateral_bps);

    let proposal = client.get_proposal(&proposal_id).unwrap();
    assert!(proposal.executed);
    assert!(!proposal.cancelled);
}

#[test]
fn test_execute_operation_fails_before_delay() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _usdc_id) = setup(&env);

    let proposal_id = client.propose_operation(
        &admin,
        &pool::AdminOperation::SetCollateralConfig(50_000_000_000, 3000),
    );

    env.ledger().with_mut(|l| l.timestamp += 10_000);

    let result = client.try_execute_operation(&admin, &proposal_id);
    assert_eq!(result, Err(Ok(PoolError::ProposalNotReady)));
}

#[test]
fn test_cancel_operation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _usdc_id) = setup(&env);

    let proposal_id = client.propose_operation(
        &admin,
        &pool::AdminOperation::SetCollateralConfig(50_000_000_000, 3000),
    );

    client.cancel_operation(&admin, &proposal_id);

    let proposal = client.get_proposal(&proposal_id).unwrap();
    assert!(proposal.cancelled);
    assert!(!proposal.executed);

    env.ledger().with_mut(|l| l.timestamp += 86_400);
    let result = client.try_execute_operation(&admin, &proposal_id);
    assert_eq!(result, Err(Ok(PoolError::ProposalAlreadyCancelled)));
}

#[test]
fn test_direct_set_collateral_config_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _usdc_id) = setup(&env);

    let result = client.try_set_collateral_config(&admin, &50_000_000_000, &3000);
    assert_eq!(result, Err(Ok(PoolError::OperationRequiresProposal)));
}

#[test]
fn test_direct_remove_token_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    let result = client.try_remove_token(&admin, &usdc_id);
    assert_eq!(result, Err(Ok(PoolError::OperationRequiresProposal)));
}

#[test]
fn test_direct_seize_collateral_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _usdc_id) = setup(&env);

    let result = client.try_seize_collateral(&admin, &1);
    assert_eq!(result, Err(Ok(PoolError::OperationRequiresProposal)));
}

#[test]
fn test_propose_remove_token() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    let proposal_id =
        client.propose_operation(&admin, &pool::AdminOperation::RemoveToken(usdc_id.clone()));

    assert_eq!(proposal_id, 1);

    let proposal = client.get_proposal(&proposal_id).unwrap();
    assert!(!proposal.executed);
    assert!(!proposal.cancelled);
}

#[test]
fn test_execute_remove_token_with_active_balances_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);
    let investor = Address::generate(&env);

    mint(&env, &usdc_id, &investor, 1_000);
    client.deposit(&investor, &usdc_id, &1_000);

    let proposal_id =
        client.propose_operation(&admin, &pool::AdminOperation::RemoveToken(usdc_id.clone()));

    env.ledger().with_mut(|l| l.timestamp += 86_400);

    let result = client.try_execute_operation(&admin, &proposal_id);
    assert_eq!(result, Err(Ok(PoolError::TokenHasActiveBalances)));
}

#[test]
fn test_execute_remove_token_succeeds_when_safe() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    let proposal_id =
        client.propose_operation(&admin, &pool::AdminOperation::RemoveToken(usdc_id.clone()));

    env.ledger().with_mut(|l| l.timestamp += 86_400);

    client.execute_operation(&admin, &proposal_id);

    let proposal = client.get_proposal(&proposal_id).unwrap();
    assert!(proposal.executed);
}

#[test]
fn test_non_admin_cannot_propose_operation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _usdc_id) = setup(&env);
    let attacker = Address::generate(&env);

    let result = client.try_propose_operation(
        &attacker,
        &pool::AdminOperation::SetCollateralConfig(50_000_000_000, 3000),
    );
    assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
}

#[test]
fn test_non_admin_cannot_execute_operation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _usdc_id) = setup(&env);
    let attacker = Address::generate(&env);

    let proposal_id = client.propose_operation(
        &admin,
        &pool::AdminOperation::SetCollateralConfig(50_000_000_000, 3000),
    );

    env.ledger().with_mut(|l| l.timestamp += 86_400);

    let result = client.try_execute_operation(&attacker, &proposal_id);
    assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
}

#[test]
fn test_non_admin_cannot_cancel_operation() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _usdc_id) = setup(&env);
    let attacker = Address::generate(&env);

    let proposal_id = client.propose_operation(
        &admin,
        &pool::AdminOperation::SetCollateralConfig(50_000_000_000, 3000),
    );

    let result = client.try_cancel_operation(&attacker, &proposal_id);
    assert_eq!(result, Err(Ok(PoolError::Unauthorized)));
}

#[test]
fn test_execute_already_executed_proposal_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _usdc_id) = setup(&env);

    let proposal_id = client.propose_operation(
        &admin,
        &pool::AdminOperation::SetCollateralConfig(50_000_000_000, 3000),
    );

    env.ledger().with_mut(|l| l.timestamp += 86_400);

    client.execute_operation(&admin, &proposal_id);

    let result = client.try_execute_operation(&admin, &proposal_id);
    assert_eq!(result, Err(Ok(PoolError::ProposalAlreadyExecuted)));
}

#[test]
fn test_cancel_already_cancelled_proposal_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _usdc_id) = setup(&env);

    let proposal_id = client.propose_operation(
        &admin,
        &pool::AdminOperation::SetCollateralConfig(50_000_000_000, 3000),
    );

    client.cancel_operation(&admin, &proposal_id);

    let result = client.try_cancel_operation(&admin, &proposal_id);
    assert_eq!(result, Err(Ok(PoolError::ProposalAlreadyCancelled)));
}

#[test]
fn test_cancel_already_executed_proposal_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _usdc_id) = setup(&env);

    let proposal_id = client.propose_operation(
        &admin,
        &pool::AdminOperation::SetCollateralConfig(50_000_000_000, 3000),
    );

    env.ledger().with_mut(|l| l.timestamp += 86_400);
    client.execute_operation(&admin, &proposal_id);

    let result = client.try_cancel_operation(&admin, &proposal_id);
    assert_eq!(result, Err(Ok(PoolError::ProposalAlreadyExecuted)));
}

#[test]
fn test_get_nonexistent_proposal_returns_none() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _admin, _usdc_id) = setup(&env);

    let proposal = client.get_proposal(&999);
    assert!(proposal.is_none());
}

#[test]
fn test_invalid_collateral_config_rejected_at_proposal_time() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _usdc_id) = setup(&env);

    // A threshold of exactly 0 is a legitimate "collateralize every invoice"
    // policy, not an error; only a negative threshold is invalid.
    let result =
        client.try_propose_operation(&admin, &pool::AdminOperation::SetCollateralConfig(-1, 3000));
    assert_eq!(result, Err(Ok(PoolError::InvalidCollateralThreshold)));

    let result = client.try_propose_operation(
        &admin,
        &pool::AdminOperation::SetCollateralConfig(50_000_000_000, 0),
    );
    assert_eq!(result, Err(Ok(PoolError::InvalidCollateralBps)));

    let result = client.try_propose_operation(
        &admin,
        &pool::AdminOperation::SetCollateralConfig(50_000_000_000, 11_000),
    );
    assert_eq!(result, Err(Ok(PoolError::InvalidCollateralBps)));
}

#[test]
fn test_set_operation_delay() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _usdc_id) = setup(&env);

    client.set_operation_delay(&admin, &7200);

    let proposal_id = client.propose_operation(
        &admin,
        &pool::AdminOperation::SetCollateralConfig(50_000_000_000, 3000),
    );

    env.ledger().with_mut(|l| l.timestamp += 7200);

    client.execute_operation(&admin, &proposal_id);

    let config = client.get_collateral_config();
    assert_eq!(config.threshold, 50_000_000_000);
}

#[test]
fn test_set_operation_delay_below_minimum_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, _usdc_id) = setup(&env);

    let result = client.try_set_operation_delay(&admin, &1800);
    assert_eq!(result, Err(Ok(PoolError::InvalidOperationDelay)));
}

#[test]
fn test_multiple_proposals_independent() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, admin, usdc_id) = setup(&env);

    let proposal_id1 = client.propose_operation(
        &admin,
        &pool::AdminOperation::SetCollateralConfig(50_000_000_000, 3000),
    );
    let proposal_id2 =
        client.propose_operation(&admin, &pool::AdminOperation::RemoveToken(usdc_id.clone()));

    assert_eq!(proposal_id1, 1);
    assert_eq!(proposal_id2, 2);

    client.cancel_operation(&admin, &proposal_id1);

    env.ledger().with_mut(|l| l.timestamp += 86_400);

    let result1 = client.try_execute_operation(&admin, &proposal_id1);
    assert_eq!(result1, Err(Ok(PoolError::ProposalAlreadyCancelled)));

    client.execute_operation(&admin, &proposal_id2);

    let prop2 = client.get_proposal(&proposal_id2).unwrap();
    assert!(prop2.executed);
}

#[test]
fn test_full_borrower_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();

    let (pool_client, admin, usdc_id) = setup(&env);

    let investor = Address::generate(&env);
    let borrower = Address::generate(&env);

    // Seed the pool with real liquidity via a proper deposit, so
    // fund_invoice's internal accounting (available_liquidity) sees funds,
    // not just the raw token balance.
    mint(&env, &usdc_id, &investor, 100_000);
    pool_client.deposit(&investor, &usdc_id, &100_000);

    let invoice_amount = 10_000i128;
    let due_date = env.ledger().timestamp() + 100_000;
    let invoice_id = 1u64;

    let borrower_balance_before = token::Client::new(&env, &usdc_id).balance(&borrower);
    let pool_balance_before = token::Client::new(&env, &usdc_id).balance(&pool_client.address);

    pool_client.fund_invoice(
        &admin,
        &invoice_id,
        &invoice_amount,
        &borrower,
        &due_date,
        &usdc_id,
    );

    let borrower_balance_after = token::Client::new(&env, &usdc_id).balance(&borrower);
    assert!(borrower_balance_after > borrower_balance_before);

    let pool_balance_after = token::Client::new(&env, &usdc_id).balance(&pool_client.address);
    assert!(pool_balance_after < pool_balance_before);

    env.ledger().with_mut(|l| l.timestamp += 10_000);

    let total_due = pool_client.estimate_repayment(&invoice_id, &None);
    mint(&env, &usdc_id, &borrower, total_due);
    pool_client.repay_invoice(&invoice_id, &borrower, &total_due);

    let pool_balance_final = token::Client::new(&env, &usdc_id).balance(&pool_client.address);
    assert!(pool_balance_final > pool_balance_after);
}
