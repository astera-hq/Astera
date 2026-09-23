use criterion::{black_box, criterion_group, criterion_main, Criterion};
use soroban_sdk::{
    testutils::{Address as _, Ledger},
    token, Address, Env, String as SorobanString, Symbol, Vec,
};

// Import contract implementations
use auction::{AuctionContract, AuctionContractClient, CollateralSaleParams};
use credit_score::{CreditScoreContract, CreditScoreContractClient};
use governance::{Governance, GovernanceAction, GovernanceClient, PoolAction, ProposalCategory};
use invoice::{InvoiceContract, InvoiceContractClient};
use pool::{FundingPool, FundingPoolClient, OpenCoFundingRequest};
use referral::{ReferralContract, ReferralContractClient};
use share::{ShareToken, ShareTokenClient};

/// Setup helper for invoice contract benchmarks
fn setup_invoice_env() -> (Env, InvoiceContractClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let contract_id = env.register(InvoiceContract, ());
    let client = InvoiceContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let pool = Address::generate(&env);

    client.initialize(&admin, &pool, &i128::MAX, &2_592_000u64, &7u32);

    (env, client, admin, pool)
}

/// Setup helper for pool contract benchmarks
fn setup_pool_env() -> (Env, FundingPoolClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();
    env.ledger().with_mut(|l| l.timestamp = 100_000);

    let contract_id = env.register(FundingPool, ());
    let client = FundingPoolClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let usdc_id = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let share_token_id = env.register(ShareToken, ());
    ShareTokenClient::new(&env, &share_token_id).initialize(
        &admin,
        &7u32,
        &SorobanString::from_str(&env, "Pool Shares"),
        &SorobanString::from_str(&env, "POOL"),
    );
    let invoice_contract = Address::generate(&env);

    // Mint USDC for testing
    soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id).mint(&admin, &10_000_000_000);

    client.initialize(&admin, &usdc_id, &share_token_id, &invoice_contract);
    client.set_max_investor_concentration(&admin, &10_000u32);

    (env, client, admin, usdc_id)
}

fn bench_create_invoice(c: &mut Criterion) {
    c.bench_function("create_invoice", |b| {
        b.iter_batched(
            || {
                let (env, client, _admin, _pool) = setup_invoice_env();
                let owner = Address::generate(&env);
                (env, client, owner)
            },
            |(env, client, owner)| {
                let debtor = SorobanString::from_str(&env, "Acme Corp");
                let amount = black_box(1_000_000_000i128);
                let due_date = black_box(env.ledger().timestamp() + 2_592_000);
                let description = SorobanString::from_str(&env, "Invoice for services");
                let verification_hash = SorobanString::from_str(&env, "hash123");
                let metadata_url = SorobanString::from_str(&env, "https://example.com/meta");

                client.create_invoice(
                    &owner,
                    &debtor,
                    &amount,
                    &due_date,
                    &description,
                    &verification_hash,
                    &metadata_url,
                )
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

fn bench_mark_paid(c: &mut Criterion) {
    c.bench_function("mark_paid", |b| {
        b.iter_batched(
            || {
                // mark_paid cross-calls the pool contract's is_invoice_repaid,
                // so this needs a real, fully-repaid pool-funded invoice
                // rather than a bare Address standing in for the pool.
                let env = Env::default();
                env.mock_all_auths_allowing_non_root_auth();
                env.ledger().with_mut(|l| l.timestamp = 100_000);

                let invoice_contract_id = env.register(InvoiceContract, ());
                let invoice_client = InvoiceContractClient::new(&env, &invoice_contract_id);
                let pool_contract_id = env.register(FundingPool, ());
                let pool_client = FundingPoolClient::new(&env, &pool_contract_id);

                let admin = Address::generate(&env);
                let token_admin = Address::generate(&env);
                let investor = Address::generate(&env);
                let usdc_id = env
                    .register_stellar_asset_contract_v2(token_admin)
                    .address();
                let share_token_id = env.register(ShareToken, ());
                ShareTokenClient::new(&env, &share_token_id).initialize(
                    &admin,
                    &7u32,
                    &SorobanString::from_str(&env, "Pool Shares"),
                    &SorobanString::from_str(&env, "POOL"),
                );

                invoice_client.initialize(
                    &admin,
                    &pool_contract_id,
                    &i128::MAX,
                    &2_592_000u64,
                    &7u32,
                );
                pool_client.initialize(&admin, &usdc_id, &share_token_id, &invoice_contract_id);
                pool_client.set_max_investor_concentration(&admin, &10_000u32);

                soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
                    .mint(&investor, &5_000_000_000);
                pool_client.deposit(&investor, &usdc_id, &5_000_000_000i128, &None);

                let owner = Address::generate(&env);
                let debtor = SorobanString::from_str(&env, "Acme Corp");
                let amount = 1_000_000_000i128;
                let due_date = env.ledger().timestamp() + 2_592_000;
                let description = SorobanString::from_str(&env, "Invoice for services");
                let verification_hash = SorobanString::from_str(&env, "hash123");
                let metadata_url = SorobanString::from_str(&env, "https://example.com/meta");

                let invoice_id = invoice_client.create_invoice(
                    &owner,
                    &debtor,
                    &amount,
                    &due_date,
                    &description,
                    &verification_hash,
                    &metadata_url,
                );
                pool_client.fund_invoice(&admin, &invoice_id, &amount, &owner, &due_date, &usdc_id);
                let total_due = pool_client.estimate_repayment(&invoice_id, &None);
                soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
                    .mint(&owner, &total_due);
                pool_client.repay_invoice(&invoice_id, &owner, &total_due);

                (env, invoice_client, invoice_id, pool_contract_id)
            },
            |(_env, client, invoice_id, pool)| {
                client.mark_paid(&black_box(invoice_id), &black_box(pool))
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

fn bench_deposit(c: &mut Criterion) {
    c.bench_function("deposit", |b| {
        b.iter_batched(
            || {
                let (env, client, _admin, usdc_id) = setup_pool_env();
                let investor = Address::generate(&env);

                // Mint USDC to investor
                soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
                    .mint(&investor, &5_000_000_000);

                (env, client, investor, usdc_id)
            },
            |(_env, client, investor, usdc_id)| {
                let amount = black_box(1_000_000_000i128);
                client.deposit(&investor, &usdc_id, &amount, &None)
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

fn bench_commit_to_invoice(c: &mut Criterion) {
    c.bench_function("commit_to_invoice", |b| {
        b.iter_batched(
            || {
                let (env, client, admin, usdc_id) = setup_pool_env();
                let investor = Address::generate(&env);
                let sme = Address::generate(&env);

                // Mint and deposit USDC
                soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id)
                    .mint(&investor, &5_000_000_000);
                client.deposit(&investor, &usdc_id, &3_000_000_000, &None);

                // Open a co-funding round
                let invoice_id = 1u64;
                let principal = 3_000_000_000i128;
                let now = env.ledger().timestamp();
                let due_date = now + 2_592_000;
                let funding_deadline = now + 1_296_000;
                client.open_co_funding(
                    &admin,
                    &OpenCoFundingRequest {
                        invoice_id,
                        token: usdc_id,
                        target_principal: principal,
                        sme,
                        due_date,
                        funding_deadline,
                        min_commitment: 0,
                        max_investor_bps: 10_000,
                    },
                );

                (env, client, investor, invoice_id)
            },
            |(_env, client, investor, invoice_id)| {
                let amount = black_box(1_000_000_000i128);
                client.commit_to_invoice(&investor, &invoice_id, &amount)
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

fn bench_repay_invoice(c: &mut Criterion) {
    c.bench_function("repay_invoice", |b| {
        b.iter_batched(
            || {
                let (env, client, admin, usdc_id) = setup_pool_env();
                let investor = Address::generate(&env);
                let sme = Address::generate(&env);

                // Mint USDC to investor and SME
                let token_client = soroban_sdk::token::StellarAssetClient::new(&env, &usdc_id);
                token_client.mint(&investor, &3_000_000_000);
                token_client.mint(&sme, &4_000_000_000);

                // Deposit, open + fill + finalize a co-funding round
                client.deposit(&investor, &usdc_id, &3_000_000_000, &None);
                let invoice_id = 1u64;
                let principal = 3_000_000_000i128;
                let now = env.ledger().timestamp();
                let due_date = now + 2_592_000;
                let funding_deadline = now + 1_296_000;
                client.open_co_funding(
                    &admin,
                    &OpenCoFundingRequest {
                        invoice_id,
                        token: usdc_id,
                        target_principal: principal,
                        sme: sme.clone(),
                        due_date,
                        funding_deadline,
                        min_commitment: 0,
                        max_investor_bps: 10_000,
                    },
                );
                client.commit_to_invoice(&investor, &invoice_id, &principal);
                client.finalize_co_funding(&admin, &invoice_id);

                // Advance time by 30 days
                env.ledger().with_mut(|l| l.timestamp += 2_592_000);

                (env, client, invoice_id, sme)
            },
            |(_env, client, invoice_id, sme)| {
                let amount = black_box(3_000_000_000i128);
                client.repay_invoice(&black_box(invoice_id), &black_box(sme), &amount)
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

// #1413: previously only invoice/pool/share had any benchmark coverage.
// These four cover the paths flagged as most likely to regress into a
// resource-limit failure on-chain rather than a test failure: a ring-buffer
// walk, two unbounded-iteration list entrypoints, and an O(n^2) re-sort.

/// credit_score's `get_credit_score` walks the full payment-history ring
/// buffer (up to MAX_PAYMENT_HISTORY records) to compute the trend
/// adjustment once an SME has at least TREND_WINDOW payments. Benchmarked
/// with the ring buffer completely full, its worst case.
fn bench_get_credit_score_full_history(c: &mut Criterion) {
    c.bench_function("credit_score_get_credit_score_full_history", |b| {
        b.iter_batched(
            || {
                let env = Env::default();
                env.mock_all_auths_allowing_non_root_auth();
                env.ledger().with_mut(|l| l.timestamp = 100_000);

                let contract_id = env.register(CreditScoreContract, ());
                let client = CreditScoreContractClient::new(&env, &contract_id);
                let admin = Address::generate(&env);
                let invoice_contract = Address::generate(&env);
                let pool_contract = Address::generate(&env);
                client.initialize(&admin, &invoice_contract, &pool_contract);

                let sme = Address::generate(&env);
                let max_history = client.get_max_payment_history();
                for i in 1..=max_history as u64 {
                    let due_date = 100_000u64;
                    client.record_payment(
                        &pool_contract,
                        &i,
                        &sme,
                        &(200_000_000i128),
                        &due_date,
                        &due_date,
                    );
                }

                (env, client, sme)
            },
            |(_env, client, sme)| client.get_credit_score(&black_box(sme)),
            criterion::BatchSize::SmallInput,
        )
    });
}

/// governance's `list_proposals` iterates every proposal ever created,
/// lazily finalizing any whose voting period has elapsed along the way —
/// unbounded in the number of proposals, with no pagination.
fn bench_governance_list_proposals(c: &mut Criterion) {
    c.bench_function("governance_list_proposals", |b| {
        b.iter_batched(
            || {
                let env = Env::default();
                env.mock_all_auths_allowing_non_root_auth();
                env.ledger().with_mut(|l| l.timestamp = 100_000);

                let share_token_id = env.register(ShareToken, ());
                let share_client = ShareTokenClient::new(&env, &share_token_id);
                let admin = Address::generate(&env);
                share_client.initialize(
                    &admin,
                    &7u32,
                    &SorobanString::from_str(&env, "Gov Shares"),
                    &SorobanString::from_str(&env, "GOV"),
                );
                let proposer = Address::generate(&env);
                share_client.mint(&proposer, &1_000_000_000i128);

                let gov_id = env.register(Governance, ());
                let gov = GovernanceClient::new(&env, &gov_id);
                gov.initialize(
                    &admin,
                    &share_token_id,
                    &0u64, // default voting period
                    &1_000u32,
                    &6_000u32,
                    &0u64,
                    &1i128,
                );

                let target = Address::generate(&env);
                for _ in 0..50 {
                    gov.create_proposal(
                        &proposer,
                        &SorobanString::from_str(&env, "Adjust pool yield"),
                        &target,
                        &GovernanceAction::Pool(PoolAction::SetPoolYield(500)),
                        &ProposalCategory::ParameterChange,
                    );
                }

                (env, gov)
            },
            |(_env, gov)| gov.list_proposals(),
            criterion::BatchSize::SmallInput,
        )
    });
}

/// auction's `list_open_sales` iterates every sale id ever opened, filtering
/// for `Open` status — unbounded in the number of sales ever created, not
/// just currently-open ones.
fn bench_auction_list_open_sales(c: &mut Criterion) {
    c.bench_function("auction_list_open_sales", |b| {
        b.iter_batched(
            || {
                let env = Env::default();
                env.mock_all_auths_allowing_non_root_auth();
                env.ledger().with_mut(|l| l.timestamp = 100_000);

                let auction_id = env.register(AuctionContract, ());
                let auction = AuctionContractClient::new(&env, &auction_id);

                let seller = Address::generate(&env);
                let token_admin = Address::generate(&env);
                let token_id = env
                    .register_stellar_asset_contract_v2(token_admin)
                    .address();
                token::StellarAssetClient::new(&env, &token_id).mint(&seller, &1_000_000_000_000);
                let proceeds_recipient = Address::generate(&env);

                for _ in 0..50 {
                    auction.open_collateral_sale(&CollateralSaleParams {
                        seller: seller.clone(),
                        token: token_id.clone(),
                        amount: 1_000_000,
                        proceeds_token: token_id.clone(),
                        proceeds_recipient: proceeds_recipient.clone(),
                        start_price: 1_000_000,
                        floor_price: 500_000,
                        duration_secs: 3_600,
                    });
                }

                (env, auction)
            },
            |(_env, auction)| auction.list_open_sales(),
            criterion::BatchSize::SmallInput,
        )
    });
}

/// referral's `update_leaderboard` (called from `record_activity` on a
/// referee's first qualifying action) re-sorts a Vec-backed leaderboard
/// capped at MAX_LEADERBOARD_SIZE (25) — O(n^2) in the worst case across
/// repeated insertions. Benchmarked with the leaderboard already full, so
/// every measured call exercises a full insertion pass.
fn bench_referral_leaderboard_insert_when_full(c: &mut Criterion) {
    c.bench_function("referral_update_leaderboard_when_full", |b| {
        b.iter_batched(
            || {
                let env = Env::default();
                env.mock_all_auths_allowing_non_root_auth();
                env.ledger().with_mut(|l| l.timestamp = 100_000);

                let referral_id = env.register(ReferralContract, ());
                let referral = ReferralContractClient::new(&env, &referral_id);
                let admin = Address::generate(&env);
                let pool = Address::generate(&env);
                referral.initialize(&admin, &pool);

                let token = Address::generate(&env);
                let kind = Symbol::new(&env, "deposit");

                // Fill the leaderboard to capacity (25 distinct referrers,
                // each activated with an increasing referral count so the
                // insert-and-resort path is fully exercised).
                for i in 0..25u32 {
                    let referrer = Address::generate(&env);
                    for j in 0..=i {
                        let referee = Address::generate(&env);
                        referral.register(&referee, &referrer);
                        referral.record_activity(&pool, &referee, &kind, &(1_000_000i128), &token);
                        let _ = j;
                    }
                }

                let new_referrer = Address::generate(&env);
                let new_referee = Address::generate(&env);
                referral.register(&new_referee, &new_referrer);

                (env, referral, pool, new_referee, kind, token)
            },
            |(_env, referral, pool, referee, kind, token)| {
                referral.record_activity(
                    &black_box(pool),
                    &black_box(referee),
                    &kind,
                    &1_000_000i128,
                    &token,
                )
            },
            criterion::BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    contract_benchmarks,
    bench_create_invoice,
    bench_mark_paid,
    bench_deposit,
    bench_commit_to_invoice,
    bench_repay_invoice,
    bench_get_credit_score_full_history,
    bench_governance_list_proposals,
    bench_auction_list_open_sales,
    bench_referral_leaderboard_insert_when_full
);
criterion_main!(contract_benchmarks);
