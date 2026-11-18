export const Errors = {
  0: { message: 'AlreadyInitialized' },
  1: { message: 'NotInitialized' },
  2: { message: 'Unauthorized' },
  3: { message: 'ContractPaused' },
  4: { message: 'ZeroAmount' },
  5: { message: 'InvalidAmount' },
  6: { message: 'ListingNotFound' },
  7: { message: 'ListingNotOpen' },
  8: { message: 'ListingNotSeller' },
  9: { message: 'TooManyListings' },
  10: { message: 'SettlementFailed' },
  11: { message: 'OrderNotFound' },
  12: { message: 'OrderNotOpen' },
  13: { message: 'OrderNotOwner' },
  14: { message: 'TooManyOrders' },
  15: { message: 'InvalidExpiry' },
} as const;

export type SecondaryMarketErrorCode = keyof typeof Errors;
export type SecondaryMarketErrorMessage = (typeof Errors)[SecondaryMarketErrorCode]['message'];
