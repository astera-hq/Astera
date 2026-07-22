import { Keypair, TransactionBuilder } from '@stellar/stellar-sdk';
import { AsteraClient } from '../../sdk/src/client'; // Direct import from source for local dev
import { OracleConfig } from './types';
import { retryWithBackoff } from './retry';

export class Verifier {
  private client: AsteraClient;
  private config: OracleConfig;
  private oracleKeypair: Keypair;
  /**
   * #861: when a registry contract is configured this node participates in
   * the N-of-M stake-weighted consensus network (`submit_vote`) instead of
   * the legacy single-oracle `verify_invoice` call. This keeps the reference
   * service able to run against either deployment model unmodified.
   */
  private readonly useConsensus: boolean;

  constructor(config: OracleConfig) {
    this.config = config;
    this.oracleKeypair = Keypair.fromSecret(config.oracleSecretKey);
    this.useConsensus = Boolean(config.oracleRegistryContractId);
    this.client = new AsteraClient({
      rpcUrl: config.rpcUrl,
      network: config.networkPassphrase,
      invoiceContractId: config.invoiceContractId,
      poolContractId: '', // Not needed for verification
      oracleRegistryContractId: config.oracleRegistryContractId,
    });
  }

  private signTx = async (xdr: string): Promise<string> => {
    const tx = TransactionBuilder.fromXDR(xdr, this.config.networkPassphrase);
    tx.sign(this.oracleKeypair);
    return tx.toXDR();
  };

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
          await this.submitVerdict(invoiceId, false, String(docError), invoice.verification_hash || '');
          return;
        }
      }

      // 3. Mock verification logic: Always verify after a delay in dev mode
      console.log(`[Verifier] Running verification logic for hash: ${invoice.verification_hash}...`);
      await new Promise(resolve => setTimeout(resolve, this.config.autoVerifyDelayMs));

      // 4. Submit this node's verdict (with retry)
      console.log(`[Verifier] Submitting verification for invoice ${invoiceId}...`);
      await this.submitVerdict(
        invoiceId,
        true,
        'Auto-verified by Reference Oracle Service',
        invoice.verification_hash || '',
      );
    } catch (error) {
      console.error(`[Verifier] Failed to verify invoice ${invoiceId}:`, error);
    }
  }

  /**
   * Submits this node's verdict for `invoiceId` — either a stake-weighted
   * vote against the registry's `VerificationRound` (opening one first if
   * none exists yet), or the legacy direct `verify_invoice` call, depending
   * on whether an oracle registry is configured.
   */
  private async submitVerdict(
    invoiceId: bigint,
    approved: boolean,
    reason: string,
    oracleHash: string,
  ): Promise<void> {
    if (!this.useConsensus) {
      const txHash = await retryWithBackoff(
        () =>
          this.client.invoice.verify({
            signer: this.signTx,
            oracle: this.oracleKeypair.publicKey(),
            id: invoiceId,
            approved,
            reason,
            oracleHash,
          }),
        `invoice.verify(${invoiceId})`,
      );
      console.log(`[Verifier] Invoice ${invoiceId} verdict (${approved}) submitted. Tx Hash: ${txHash}`);
      return;
    }

    // Ensure a verification round is open before voting. `open_verification_round`
    // is idempotent from this node's point of view: if another oracle already
    // opened it (or already finalized it), the "already open"/"not found"-style
    // failure is expected and safely ignored — the vote attempt right after
    // will surface any real problem (e.g. the round already finalized).
    const existingRound = await this.client.oracleRegistry.getRound(invoiceId).catch(() => null);
    if (!existingRound || existingRound.status !== 'Open') {
      try {
        await retryWithBackoff(
          () =>
            this.client.oracleRegistry.openRound({
              signer: this.signTx,
              caller: this.oracleKeypair.publicKey(),
              invoiceId,
              oracleHash,
            }),
          `oracleRegistry.openRound(${invoiceId})`,
        );
      } catch (openError) {
        console.log(
          `[Verifier] Could not open round for invoice ${invoiceId} (likely already open): ${openError}`,
        );
      }
    }

    const txHash = await retryWithBackoff(
      () =>
        this.client.oracleRegistry.vote({
          signer: this.signTx,
          oracle: this.oracleKeypair.publicKey(),
          invoiceId,
          approved,
          evidenceHash: oracleHash,
        }),
      `oracleRegistry.vote(${invoiceId})`,
    );
    console.log(`[Verifier] Vote (${approved}) submitted for invoice ${invoiceId}. Tx Hash: ${txHash}`);
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
}
