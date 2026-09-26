use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
#[repr(u32)]
pub enum TrancheError {
    AlreadyInitialized = 1,
    Unauthorized = 2,
    PoolNotFound = 3,
    InvalidAmount = 4,
    InsufficientBalance = 5,
    AdvanceRateExceeded = 6,
    ReentrancyDetected = 11,
    NotInitialized = 12,
    ExposureNotFound = 13,
    ContractPaused = 14,
}
