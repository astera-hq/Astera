export const Errors = {
  0: { message: 'AlreadyInitialized' },
  1: { message: 'NotInitialized' },
  2: { message: 'Unauthorized' },
  3: { message: 'RoundNotFound' },
  4: { message: 'RoundNotOpen' },
  5: { message: 'RoundAlreadySettled' },
  6: { message: 'BidWindowExpired' },
  7: { message: 'InvoiceNotVerified' },
  8: { message: 'InvoiceAlreadyBid' },
  9: { message: 'DiscountTooHigh' },
  10: { message: 'NoBids' },
  11: { message: 'InsufficientLiquidity' },
  12: { message: 'AllocationOverflow' },
  13: { message: 'RoundNotClearing' },
  14: { message: 'PoolCallFailed' },
  // #1036: collateral-liquidation sale errors
  15: { message: 'InvalidSaleParams' },
  16: { message: 'SaleNotFound' },
  17: { message: 'SaleNotOpen' },
  18: { message: 'SaleExpired' },
  19: { message: 'SaleNotExpired' },
  // #1036: collateral-risk-response errors
  20: { message: 'DepositNotFound' },
  21: { message: 'InvoiceNotFound' },
  22: { message: 'CollateralAlreadySettled' },
  23: { message: 'OraclePriceUnavailable' },
  24: { message: 'NotAtRisk' },
  25: { message: 'GracePeriodNotElapsed' },
  26: { message: 'InvalidRiskConfig' },
  27: { message: 'AmountOverflow' },
} as const;

export type AuctionErrorCode = keyof typeof Errors;
export type AuctionErrorMessage = (typeof Errors)[AuctionErrorCode]['message'];
