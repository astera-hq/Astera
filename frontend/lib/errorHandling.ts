const ERROR_MAPPINGS: Array<{ match: RegExp; message: string }> = [
  { match: /USER_DECLINED_ACCESS|user rejected|cancelled by user/i, message: 'Transaction cancelled by user' },
  { match: /timeout|network error|failed to fetch|unreachable/i, message: 'Network error, please try again' },
  { match: /InsufficientFunds|insufficient funds/i, message: 'Insufficient balance for this transaction' },
  { match: /ContractPaused|contract is paused/i, message: 'Protocol is currently paused' },
  { match: /already initialized/i, message: 'Contract is already initialized' },
];

export function getUserFriendlyError(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error ?? 'Unknown error');
  const mapping = ERROR_MAPPINGS.find((item) => item.match.test(raw));
  return mapping?.message ?? raw;
}
