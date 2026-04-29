export interface OracleConfig {
  rpcUrl: string;
  horizonUrl: string;
  networkPassphrase: string;
  oracleSecretKey: string;
  invoiceContractId: string;
  autoVerifyDelayMs: number;
}

export interface InvoiceCreatedEvent {
  id: bigint;
  owner: string;
  amount: bigint;
  metadataUri?: string;
}
