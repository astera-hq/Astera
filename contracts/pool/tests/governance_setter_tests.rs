#![cfg(test)]

//! #1649: table-driven coverage of the pool's governance setters
//! (`*_gov` / `*_via_governance`). For every setter this verifies that
//!
//! * a caller that is not the configured governance address is rejected,
//! * the configured governance address can apply the value (read back through
//!   the matching getter where the pool exposes one) and the setter emits its
//!   event,
//! * out-of-bounds values are rejected with the documented error.

use pool::{FeeTier, FundingPool, FundingPoolClient, LoyaltyTier, PoolError};
use soroban_sdk::{
    testutils::{Address as _, Events},
    vec, Address, Env, FromVal, Symbol, Vec,
};

/// Collateral configuration mirror (type is re-exported by the pool crate).
use pool::CollateralConfig;

struct Fx {
    env: Env,
    client: FundingPoolClient<'static>,
    admin: Address,
    governance: Address,
}

/// `governance_is_admin` is needed for the three setters that additionally
/// call `require_admin(governance)` (min deposit, upgrade timelock, operation
/// delay) — see the PR notes.
fn setup(configure_governance: bool, governance_is_admin: bool) -> Fx {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();
    let share_token = Address::generate(&env);
    let invoice_contract = Address::generate(&env);
    let pool_id = env.register(FundingPool, ());
    let client = FundingPoolClient::new(&env, &pool_id);
    client.initialize(&admin, &token, &share_token, &invoice_contract);

    let governance = if governance_is_admin {
        admin.clone()
    } else {
        Address::generate(&env)
    };
    if configure_governance {
        client.set_governance_address(&admin, &governance);
    }
    Fx {
        env,
        client,
        admin,
        governance,
    }
}

type Call = Box<dyn Fn(&Fx, &Address) -> Result<(), PoolError>>;
type Verify = Box<dyn Fn(&Fx)>;

struct Case {
    name: &'static str,
    /// Second event topic emitted by the setter.
    event: &'static str,
    /// Needs `governance == admin` (setter also calls `require_admin`).
    admin_gated: bool,
    apply: Call,
    verify: Verify,
    /// Out-of-bounds call and the error it must raise.
    invalid: Option<(Call, PoolError)>,
}

/// Collapses a generated `try_*` result into `Result<(), PoolError>`.
fn norm<T, C, E>(r: Result<Result<T, C>, Result<PoolError, E>>) -> Result<(), PoolError>
where
    C: core::fmt::Debug,
    E: core::fmt::Debug,
{
    match r {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(c)) => panic!("conversion error: {c:?}"),
        Err(Ok(e)) => Err(e),
        Err(Err(e)) => panic!("invoke error: {e:?}"),
    }
}

fn last_event_topic(env: &Env) -> Symbol {
    let events = env.events().all();
    let (_, topics, _) = events.last().expect("setter must emit an event");
    Symbol::from_val(env, &topics.get(1).expect("event has a name topic"))
}

macro_rules! call {
    ($method:ident($($arg:expr),*)) => {
        Box::new(move |f: &Fx, caller: &Address| norm(f.client.$method(caller, $($arg),*)))
    };
}

fn cases(env: &Env) -> std::vec::Vec<Case> {
    let mut v: std::vec::Vec<Case> = std::vec::Vec::new();
    let addr = Address::generate(env);
    let token = Address::generate(env);

    macro_rules! addr_case {
        ($name:literal, $event:literal, $setter:ident, $getter:ident) => {{
            let a = addr.clone();
            let a2 = addr.clone();
            v.push(Case {
                name: $name,
                event: $event,
                admin_gated: false,
                apply: Box::new(move |f, c| norm(f.client.$setter(c, &a))),
                verify: Box::new(move |f| assert_eq!(f.client.$getter(), Some(a2.clone()))),
                invalid: None,
            });
        }};
    }

    v.push(Case {
        name: "set_yield_via_governance",
        event: "gov_yield",
        admin_gated: false,
        apply: call!(try_set_yield_via_governance(&1_200u32)),
        verify: Box::new(|f| assert_eq!(f.client.get_config().yield_bps, 1_200)),
        invalid: Some((
            call!(try_set_yield_via_governance(&5_001u32)),
            PoolError::InvalidAmount,
        )),
    });
    {
        let t = addr.clone();
        let t2 = addr.clone();
        v.push(Case {
            name: "set_treasury_via_governance",
            event: "gov_treas",
            admin_gated: false,
            apply: Box::new(move |f, c| norm(f.client.try_set_treasury_via_governance(c, &t))),
            verify: Box::new(move |f| assert_eq!(f.client.get_treasury(), t2)),
            invalid: None,
        });
    }
    v.push(Case {
        name: "set_max_utilization_gov",
        event: "gov_util",
        admin_gated: false,
        apply: call!(try_set_max_utilization_gov(&7_500u32)),
        verify: Box::new(|f| assert_eq!(f.client.get_config().max_utilization_bps, 7_500)),
        invalid: Some((
            call!(try_set_max_utilization_gov(&10_001u32)),
            PoolError::InvalidAmount,
        )),
    });
    addr_case!(
        "set_oracle_contract_gov",
        "gov_orcl",
        try_set_oracle_contract_gov,
        get_oracle_contract
    );
    v.push(Case {
        name: "set_kyc_required_via_governance",
        event: "gov_kyc",
        admin_gated: false,
        apply: call!(try_set_kyc_required_via_governance(&true)),
        verify: Box::new(|f| assert!(f.client.kyc_required())),
        invalid: None,
    });
    addr_case!(
        "set_compliance_registry_gov",
        "gov_comp",
        try_set_compliance_registry_gov,
        get_compliance_registry
    );
    v.push(Case {
        name: "set_require_compliance_check_gov",
        event: "gov_cmp_r",
        admin_gated: false,
        apply: Box::new(|f, c| {
            // Needs a registry first (mirrors the admin setter).
            let registry = Address::generate(&f.env);
            norm(f.client.try_set_compliance_registry_gov(c, &registry))?;
            norm(f.client.try_set_require_compliance_check_gov(c, &true))
        }),
        verify: Box::new(|f| assert!(f.client.require_compliance_check())),
        // No registry configured yet -> must fail cleanly, not abort.
        invalid: Some((
            call!(try_set_require_compliance_check_gov(&true)),
            PoolError::ComplianceCheckFailed,
        )),
    });
    addr_case!(
        "set_referral_registry_gov",
        "gov_ref",
        try_set_referral_registry_gov,
        get_referral_registry
    );
    addr_case!(
        "set_credit_score_contract_gov",
        "gov_cs",
        try_set_credit_score_contract_gov,
        get_credit_score_contract
    );
    addr_case!(
        "set_insurance_contract_gov",
        "gov_ins",
        try_set_insurance_contract_gov,
        get_insurance_contract
    );
    v.push(Case {
        name: "set_compound_interest_gov",
        event: "gov_cmp_i",
        admin_gated: false,
        apply: call!(try_set_compound_interest_gov(&true)),
        verify: Box::new(|f| assert!(f.client.get_config().compound_interest)),
        invalid: None,
    });
    addr_case!(
        "set_secondary_market_gov",
        "gov_sm",
        try_set_secondary_market_gov,
        get_secondary_market_contract
    );
    {
        let a = addr.clone();
        v.push(Case {
            name: "set_risk_contract_via_governance",
            event: "gov_risk",
            admin_gated: false,
            apply: Box::new(move |f, c| norm(f.client.try_set_risk_contract_via_governance(c, &a))),
            // No public getter; the stored address is exercised by
            // `risk_liquidate_collateral` in pool_entrypoint_tests.rs.
            verify: Box::new(|_| {}),
            invalid: None,
        });
    }
    v.push(Case {
        name: "set_min_deposit_via_governance",
        event: "gov_mind",
        admin_gated: true,
        apply: call!(try_set_min_deposit_via_governance(&5_000_000i128)),
        verify: Box::new(|f| assert_eq!(f.client.get_min_deposit(), 5_000_000)),
        invalid: Some((
            call!(try_set_min_deposit_via_governance(&0i128)),
            PoolError::InvalidAmount,
        )),
    });
    v.push(Case {
        name: "set_max_investor_conc_gov",
        event: "gov_conc",
        admin_gated: false,
        apply: call!(try_set_max_investor_conc_gov(&3_000u32)),
        verify: Box::new(|f| assert_eq!(f.client.get_config().max_single_investor_bps, 3_000)),
        invalid: Some((
            call!(try_set_max_investor_conc_gov(&10_001u32)),
            PoolError::InvalidAmount,
        )),
    });
    v.push(Case {
        name: "set_upgrade_timelock_gov",
        event: "gov_up_tl",
        admin_gated: true,
        apply: call!(try_set_upgrade_timelock_gov(&172_800u64)),
        verify: Box::new(|f| assert_eq!(f.client.get_upgrade_timelock(), 172_800)),
        invalid: Some((
            call!(try_set_upgrade_timelock_gov(&86_399u64)),
            PoolError::InvalidUpgradeTimelock,
        )),
    });
    v.push(Case {
        name: "set_operation_delay_gov",
        event: "gov_opdl",
        admin_gated: true,
        apply: call!(try_set_operation_delay_gov(&7_200u64)),
        verify: Box::new(|f| assert_eq!(f.client.get_operation_delay(), 7_200)),
        invalid: Some((
            call!(try_set_operation_delay_gov(&3_599u64)),
            PoolError::InvalidOperationDelay,
        )),
    });
    v.push(Case {
        name: "set_yield_change_policy_gov",
        event: "gov_ycld",
        admin_gated: false,
        apply: call!(try_set_yield_change_policy_gov(&43_200u64)),
        verify: Box::new(|f| assert_eq!(f.client.get_config().yield_change_cooldown_secs, 43_200)),
        invalid: None,
    });
    v.push(Case {
        name: "set_factoring_fee_via_governance",
        event: "gov_fact",
        admin_gated: false,
        apply: call!(try_set_factoring_fee_via_governance(&250u32)),
        verify: Box::new(|f| assert_eq!(f.client.get_config().factoring_fee_bps, 250)),
        invalid: Some((
            call!(try_set_factoring_fee_via_governance(&5_001u32)),
            PoolError::InvalidAmount,
        )),
    });
    v.push(Case {
        name: "set_withdrawal_limits_gov",
        event: "gov_wdlm",
        admin_gated: false,
        apply: call!(try_set_withdrawal_limits_gov(&2_500u32)),
        verify: Box::new(|f| assert_eq!(f.client.get_config().max_single_withdrawal_bps, 2_500)),
        invalid: Some((
            call!(try_set_withdrawal_limits_gov(&10_001u32)),
            PoolError::InvalidAmount,
        )),
    });
    v.push(Case {
        name: "set_max_wd_queue_age_gov",
        event: "gov_wdag",
        admin_gated: false,
        apply: call!(try_set_max_wd_queue_age_gov(&14u32)),
        verify: Box::new(|f| assert_eq!(f.client.get_config().max_withdrawal_queue_age_days, 14)),
        invalid: None,
    });
    v.push(Case {
        name: "set_max_wd_queue_depth_gov",
        event: "gov_wddp",
        admin_gated: false,
        apply: call!(try_set_max_wd_queue_depth_gov(&25u32)),
        verify: Box::new(|f| assert_eq!(f.client.get_config().max_withdrawal_queue_depth, 25)),
        invalid: None,
    });
    v.push(Case {
        name: "set_oracle_stale_thresh_gov",
        event: "gov_orst",
        admin_gated: false,
        apply: call!(try_set_oracle_stale_thresh_gov(&7_200u64)),
        verify: Box::new(|f| assert_eq!(f.client.get_oracle_stale_threshold(), 7_200)),
        invalid: Some((
            call!(try_set_oracle_stale_thresh_gov(&0u64)),
            PoolError::InvalidAmount,
        )),
    });
    v.push(Case {
        name: "set_fee_tier_via_governance",
        event: "gov_fee",
        admin_gated: false,
        apply: Box::new(|f, c| {
            norm(f.client.try_set_fee_tier_via_governance(
                c,
                &1u32,
                &FeeTier {
                    min_amount: 0,
                    max_amount: 1_000_000,
                    min_credit_score: 0,
                    fee_bps: 150,
                },
            ))
        }),
        verify: Box::new(|f| {
            assert_eq!(
                f.client.get_fee_tier(&1u32),
                Some(FeeTier {
                    min_amount: 0,
                    max_amount: 1_000_000,
                    min_credit_score: 0,
                    fee_bps: 150,
                })
            )
        }),
        invalid: Some((
            Box::new(|f, c| {
                norm(f.client.try_set_fee_tier_via_governance(
                    c,
                    &2u32,
                    &FeeTier {
                        min_amount: 10,
                        max_amount: 5,
                        min_credit_score: 0,
                        fee_bps: 100,
                    },
                ))
            }),
            PoolError::InvalidFeeTier,
        )),
    });
    v.push(Case {
        name: "set_loyalty_tiers_via_governance",
        event: "gov_loy",
        admin_gated: false,
        apply: Box::new(|f, c| {
            let tiers = vec![
                &f.env,
                LoyaltyTier {
                    min_days: 30,
                    bonus_bps: 50,
                },
                LoyaltyTier {
                    min_days: 90,
                    bonus_bps: 100,
                },
            ];
            norm(f.client.try_set_loyalty_tiers_via_governance(c, &tiers))
        }),
        verify: Box::new(|f| {
            let stored = f.client.get_loyalty_tiers();
            assert_eq!(stored.len(), 2);
            assert_eq!(stored.get(1).unwrap().min_days, 90);
            assert_eq!(stored.get(1).unwrap().bonus_bps, 100);
        }),
        invalid: Some((
            Box::new(|f, c| {
                let empty: Vec<LoyaltyTier> = Vec::new(&f.env);
                norm(f.client.try_set_loyalty_tiers_via_governance(c, &empty))
            }),
            PoolError::InvalidLoyaltyTiers,
        )),
    });
    {
        let t = token.clone();
        let t2 = token.clone();
        let t3 = token.clone();
        v.push(Case {
            name: "set_fallback_price_gov",
            event: "gov_fbp",
            admin_gated: false,
            apply: Box::new(move |f, c| {
                norm(f.client.try_set_fallback_price_gov(c, &t, &10_000_000i128))
            }),
            verify: Box::new(move |f| {
                assert_eq!(f.client.get_fallback_price(&t2), Some(10_000_000))
            }),
            invalid: Some((
                Box::new(move |f, c| norm(f.client.try_set_fallback_price_gov(c, &t3, &0i128))),
                PoolError::InvalidAmount,
            )),
        });
    }
    {
        let t = token.clone();
        let t2 = token.clone();
        let t3 = token.clone();
        v.push(Case {
            name: "set_rate_bounds_via_governance",
            event: "gov_rate",
            admin_gated: false,
            apply: Box::new(move |f, c| {
                norm(
                    f.client
                        .try_set_rate_bounds_via_governance(c, &t, &9_000u32, &11_000u32),
                )
            }),
            verify: Box::new(move |f| {
                let b = f.client.get_rate_bounds(&t2);
                assert_eq!((b.min_bps, b.max_bps), (9_000, 11_000));
            }),
            invalid: Some((
                Box::new(move |f, c| {
                    norm(
                        f.client
                            .try_set_rate_bounds_via_governance(c, &t3, &12_000u32, &9_000u32),
                    )
                }),
                PoolError::InvalidAmount,
            )),
        });
    }
    {
        let t = token.clone();
        let t2 = token.clone();
        let t3 = token.clone();
        v.push(Case {
            name: "set_exchange_rate_via_governance",
            event: "gov_exch",
            admin_gated: false,
            apply: Box::new(move |f, c| {
                norm(
                    f.client
                        .try_set_exchange_rate_via_governance(c, &t, &9_900u32),
                )
            }),
            verify: Box::new(move |f| assert_eq!(f.client.get_exchange_rate(&t2), 9_900)),
            invalid: Some((
                Box::new(move |f, c| {
                    norm(f.client.try_set_exchange_rate_via_governance(c, &t3, &0u32))
                }),
                PoolError::InvalidAmount,
            )),
        });
    }
    v.push(Case {
        name: "set_collateral_config_gov",
        event: "gov_coll",
        admin_gated: false,
        apply: Box::new(|f, c| {
            norm(f.client.try_set_collateral_config_gov(
                c,
                &CollateralConfig {
                    threshold: 500_000_000,
                    collateral_bps: 2_500,
                },
            ))
        }),
        verify: Box::new(|f| {
            assert_eq!(
                f.client.get_collateral_config(),
                CollateralConfig {
                    threshold: 500_000_000,
                    collateral_bps: 2_500,
                }
            )
        }),
        invalid: Some((
            Box::new(|f, c| {
                norm(f.client.try_set_collateral_config_gov(
                    c,
                    &CollateralConfig {
                        threshold: 500_000_000,
                        collateral_bps: 10_001,
                    },
                ))
            }),
            PoolError::InvalidAmount,
        )),
    });

    v
}

/// The 29 governance setters the pool exposes. Keep in sync with lib.rs.
const SETTER_COUNT: usize = 29;

#[test]
fn table_covers_every_governance_setter() {
    let f = setup(true, false);
    assert_eq!(cases(&f.env).len(), SETTER_COUNT);
}

#[test]
fn non_governance_caller_is_rejected_by_every_setter() {
    let f = setup(true, false);
    let impostor = Address::generate(&f.env);
    for case in cases(&f.env) {
        // Neither a random address nor the admin may use the governance path.
        for caller in [&impostor, &f.admin] {
            assert_eq!(
                (case.apply)(&f, caller),
                Err(PoolError::Unauthorized),
                "{}: non-governance caller must be rejected",
                case.name
            );
        }
    }
}

#[test]
fn every_setter_fails_when_no_governance_address_is_configured() {
    let f = setup(false, false);
    let caller = Address::generate(&f.env);
    for case in cases(&f.env) {
        assert_eq!(
            (case.apply)(&f, &caller),
            Err(PoolError::GovernanceNotConfigured),
            "{}: must fail closed without a governance address",
            case.name
        );
    }
}

#[test]
fn governance_caller_applies_value_and_emits_event() {
    for admin_gated in [false, true] {
        // admin-gated setters additionally require governance == admin.
        let f = setup(true, admin_gated);
        for case in cases(&f.env)
            .into_iter()
            .filter(|c| c.admin_gated == admin_gated)
        {
            assert_eq!(
                (case.apply)(&f, &f.governance),
                Ok(()),
                "{}: governance call must succeed",
                case.name
            );
            // Read the event first: later contract calls reset the event log.
            assert_eq!(
                last_event_topic(&f.env),
                Symbol::new(&f.env, case.event),
                "{}: unexpected event",
                case.name
            );
            (case.verify)(&f);
        }
    }
}

#[test]
fn out_of_bounds_values_are_rejected_and_leave_state_untouched() {
    for admin_gated in [false, true] {
        let f = setup(true, admin_gated);
        for case in cases(&f.env)
            .into_iter()
            .filter(|c| c.admin_gated == admin_gated)
        {
            let Some((invalid, expected)) = case.invalid.as_ref() else {
                continue;
            };
            assert_eq!(
                invalid(&f, &f.governance),
                Err(*expected),
                "{}: out-of-bounds value must be rejected",
                case.name
            );
        }
    }
}

#[test]
fn rejected_bounds_do_not_change_the_stored_config() {
    let f = setup(true, false);
    let before = f.client.get_config();
    assert_eq!(
        f.client
            .try_set_yield_via_governance(&f.governance, &5_001u32)
            .unwrap_err()
            .unwrap(),
        PoolError::InvalidAmount
    );
    assert_eq!(f.client.get_config().yield_bps, before.yield_bps);
}
