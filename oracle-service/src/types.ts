export interface OracleConfig {
  rpcUrl: string;
  horizonUrl: string;
  networkPassphrase: string;
  oracleSecretKey: string;
  invoiceContractId: string;
  autoVerifyDelayMs: number;
  // #861: N-of-M staked oracle consensus network. When `oracleRegistryContractId`
  // is set, this node runs as one voter in the network (submitting stake-weighted
  // votes via `submit_vote`) instead of the legacy single-oracle `verify_invoice`
  // call. `minStake`/`registerStakeAmount` are only used by the `--register`
  // startup flag (see `staking.ts`).
  oracleRegistryContractId?: string;
  stakeTokenId?: string;
  registerStakeAmount?: bigint;
}

export interface InvoiceCreatedEvent {
  id: bigint;
  owner: string;
  amount: bigint;
  metadataUri?: string;
}

// #861: mirrors `RoundStatus` in contracts/oracle_registry/src/lib.rs.
export type RoundStatus = 'Open' | 'ConsensusApproved' | 'ConsensusRejected' | 'Expired';
