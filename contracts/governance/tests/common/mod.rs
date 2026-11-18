#![cfg(test)]
#![allow(dead_code)]

//! A minimal stand-in "target" contract for governance integration tests.
//!
//! The real target contracts (pool, invoice, oracle_registry, compliance)
//! are independently broken on `main` for reasons unrelated to governance
//! (missing storage keys, moved-value bugs, and — for compliance's two
//! `*_via_governance` methods specifically — contract function names that
//! exceed Soroban's 32-character limit, which means no valid contract can
//! ever implement them as written). Pulling those crates in as governance
//! test dependencies would make governance's own test suite hostage to
//! unrelated, pre-existing bugs in other crates.
//!
//! This mock instead implements the exact function signatures declared by
//! governance's own `PoolContract`, `InvoiceContract`, and
//! `OracleRegistryContract` client traits (picking the subset of methods
//! whose names are short enough to actually compile), so tests can register
//! it as a real deployed contract and prove `execute_proposal` performs a
//! genuine cross-contract call (#1119) rather than merely emitting an event.

use soroban_sdk::{contract, contractimpl, symbol_short, Address, Env};

#[contract]
pub struct MockTarget;

#[contractimpl]
impl MockTarget {
    // Mirrors `PoolContract::set_yield_via_governance`.
    pub fn set_yield_via_governance(env: Env, governance: Address, new_yield_bps: u32) {
        governance.require_auth();
        env.storage()
            .instance()
            .set(&symbol_short!("yield"), &new_yield_bps);
    }

    pub fn get_yield(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("yield"))
            .unwrap_or(0)
    }

    // Mirrors `InvoiceContract::set_grace_period_via_governance`.
    pub fn set_grace_period_via_governance(env: Env, governance: Address, days: u32) {
        governance.require_auth();
        env.storage().instance().set(&symbol_short!("grace"), &days);
    }

    pub fn get_grace_period(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&symbol_short!("grace"))
            .unwrap_or(0)
    }

    // Mirrors `OracleRegistryContract::set_treasury_via_governance`.
    pub fn set_treasury_via_governance(env: Env, governance: Address, treasury: Option<Address>) {
        governance.require_auth();
        env.storage()
            .instance()
            .set(&symbol_short!("treasury"), &treasury);
    }

    pub fn get_treasury(env: Env) -> Option<Address> {
        env.storage().instance().get(&symbol_short!("treasury"))
    }
}
