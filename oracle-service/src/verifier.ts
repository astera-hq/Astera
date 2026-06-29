import { Keypair, TransactionBuilder } from '@stellar/stellar-sdk';
import { AsteraClient } from '../../sdk/src/client'; // Direct import from source for local dev
import { OracleConfig } from './types';
import { retryWithBackoff } from './retry';

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
      // 1. Fetch invoice details (with retry)
      const invoice = await retryWithBackoff(
        () => this.client.invoice.get(invoiceId),
        `invoice.get(${invoiceId})`,
      );
      console.log(`[Verifier] Invoice ${invoiceId} data:`, invoice);

      // 2. Fetch and verify metadata if exists
      if (invoice.metadata_uri) {
        console.log(`[Verifier] Downloading document from ${invoice.metadata_uri}... (mock)`);

        // Simulate document verification with possible failure scenarios
        try {
          const docVerified = await this.verifyDocument(invoice.metadata_uri, invoice.verification_hash);
          if (!docVerified) {
            throw new Error('Document verification failed: hash mismatch');
          }
        } catch (docError) {
          console.error(`[Verifier] Permanent verification failure for invoice ${invoiceId}:`, docError);
          await this.markDisputedOnChain(invoiceId, String(docError));
          return;
        }
      }

      // 3. Mock verification logic: Always verify after a delay in dev mode
      console.log(`[Verifier] Running verification logic for hash: ${invoice.verification_hash}...`);
      await new Promise(resolve => setTimeout(resolve, this.config.autoVerifyDelayMs));

      // 4. Submit verification to contract with approved status (with retry)
      console.log(`[Verifier] Submitting verification for invoice ${invoiceId}...`);
      const txHash = await retryWithBackoff(
        () =>
          this.client.invoice.verify({
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
          }),
        `invoice.verify(${invoiceId})`,
      );

      console.log(`[Verifier] Invoice ${invoiceId} verified successfully. Tx Hash: ${txHash}`);
    } catch (error) {
      console.error(`[Verifier] Failed to verify invoice ${invoiceId}:`, error);
    }
  }

  private async verifyDocument(uri: string, expectedHash?: string): Promise<boolean> {
    // In a real implementation, this would:
    // 1. Download the document from the URI
    // 2. Compute its hash
    // 3. Compare with expectedHash
    // 4. Return true if match, false if mismatch
    // 5. Throw if document not found or unreachable

    if (!uri) {
      throw new Error('Document URI is empty');
    }

    // Mock: simulate successful verification
    return true;
  }

  private async markDisputedOnChain(invoiceId: bigint, reason: string): Promise<void> {
    try {
      console.log(`[Verifier] Marking invoice ${invoiceId} as disputed: ${reason}`);
      const txHash = await retryWithBackoff(
        () =>
          this.client.invoice.verify({
            signer: async (xdr) => {
              const tx = TransactionBuilder.fromXDR(xdr, this.config.networkPassphrase);
              tx.sign(this.oracleKeypair);
              return tx.toXDR();
            },
            oracle: this.oracleKeypair.publicKey(),
            id: invoiceId,
            approved: false,
            reason,
            oracleHash: '',
          }),
        `invoice.verify(dispute:${invoiceId})`,
      );
      console.log(`[Verifier] Invoice ${invoiceId} marked as disputed. Tx Hash: ${txHash}`);
    } catch (error) {
      console.error(`[Verifier] Failed to mark invoice ${invoiceId} as disputed:`, error);
      throw error;
    }
  }
}
