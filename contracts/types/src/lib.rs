#![no_std]

//! Contract payloads shared by the contracts that exchange typed Soroban values.
//! Keeping these definitions in one crate makes field drift a compile-time error.

use soroban_sdk::contracttype;

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct LoyaltyTier {
    pub min_days: u32,
    pub bonus_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct FeeTier {
    pub min_amount: i128,
    pub max_amount: i128,
    pub min_credit_score: u32,
    pub fee_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct CollateralConfig {
    pub threshold: i128,
    pub collateral_bps: u32,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct QuorumTier {
    pub min_invoice_amount: i128,
    pub quorum_bps: u32,
}
