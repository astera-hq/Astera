import { Keypair, TransactionBuilder } from '@stellar/stellar-sdk';
import { AsteraClient } from '../../sdk/src/client'; // Direct import from source for local dev
import { OracleConfig } from './types';

export class Verifier {
  private client: AsteraClient;
  private config: OracleConfig;
  private oracleKeypair: Keypair;

  constructor(config: OracleConfig) {
    this.config = config;
    this.oracleKeypair = Keypair.fromSecret(config.oracleSecretKey);
    this.client = new AsteraClient({
      rpcUrl: config.rpcUrl,
      network: config.networkPassphrase,
      invoiceContractId: config.invoiceContractId,
      poolContractId: '', // Not needed for verification
    });
  }

  async verifyInvoice(invoiceId: bigint) {
    console.log(`[Verifier] Starting verification for invoice ${invoiceId}...`);

    try {
      // 1. Fetch invoice details
      const invoice = await this.client.invoice.get(invoiceId);
      console.log(`[Verifier] Invoice ${invoiceId} data:`, invoice);

      // 2. Fetch metadata if exists
      if (invoice.metadata_uri) {
        // In a real scenario, we would download and verify the document at metadata_uri
        console.log(`[Verifier] Downloading document from ${invoice.metadata_uri}... (mock)`);
      }

      // 3. Mock verification logic: Always verify after a delay in dev mode
      console.log(`[Verifier] Running verification logic for hash: ${invoice.verification_hash}...`);
      await new Promise(resolve => setTimeout(resolve, this.config.autoVerifyDelayMs));

      // 4. Submit verification to contract
      console.log(`[Verifier] Submitting verification for invoice ${invoiceId}...`);
      const txHash = await this.client.invoice.verify({
        signer: async (xdr) => {
          const tx = TransactionBuilder.fromXDR(xdr, this.config.networkPassphrase);
          tx.sign(this.oracleKeypair);
          return tx.toXDR();
        },
        oracle: this.oracleKeypair.publicKey(),
        id: invoiceId,
        approved: true,
        reason: 'Auto-verified by Reference Oracle Service',
        oracleHash: invoice.verification_hash || '',
      });

      console.log(`[Verifier] Invoice ${invoiceId} verified successfully. Tx Hash: ${txHash}`);
    } catch (error) {
      console.error(`[Verifier] Failed to verify invoice ${invoiceId}:`, error);
    }
  }
}
