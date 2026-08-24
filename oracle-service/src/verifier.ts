import { createHash } from 'crypto';
import { Keypair, TransactionBuilder } from '@stellar/stellar-sdk';
import { AsteraClient, OracleRegistryErrors } from 'astera-sdk';
import { OracleConfig } from './types';
import { retryWithBackoff } from './retry';
import { ConsensusTracker } from './consensus';

// The registry surfaces a paused contract as a simulation failure containing
// `Error(Contract, #<code>)`, where `<code>` is the `ContractPaused` variant
// from the generated bindings (kept in sync with contracts/oracle_registry).
const CONTRACT_PAUSED_CODE = Object.entries(OracleRegistryErrors).find(
  ([, entry]) => entry.message === 'ContractPaused',
)?.[0];

function isRegistryPausedError(error: unknown): boolean {
  if (CONTRACT_PAUSED_CODE === undefined) {
    return false;
  }
  const message = error instanceof Error ? error.message : String(error);
  return message.includes(`Error(Contract, #${CONTRACT_PAUSED_CODE})`);
}

export class Verifier {
  private client: AsteraClient;
  private config: OracleConfig;
  private oracleKeypair: Keypair;
  private consensusTracker?: ConsensusTracker;
  /**
   * #861: when a registry contract is configured this node participates in
   * the N-of-M stake-weighted consensus network (`submit_vote`) instead of
   * the legacy single-oracle `verify_invoice` call. This keeps the reference
   * service able to run against either deployment model unmodified.
   */
  private readonly useConsensus: boolean;

  constructor(config: OracleConfig, consensusTracker?: ConsensusTracker) {
    this.config = config;
    this.consensusTracker = consensusTracker;
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

    // If the registry is known to be paused, don't bother attempting the
    // round-open/vote calls at all — they will only fail with `ContractPaused`.
    // Submission resumes automatically once an `unpaused` event is observed.
    if (this.consensusTracker?.isPaused()) {
      console.warn(
        `[Verifier] Oracle registry is paused; skipping vote submission for invoice ${invoiceId} until it resumes.`,
      );
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
        if (isRegistryPausedError(openError)) {
          this.consensusTracker?.markPaused();
          console.warn(
            `[Verifier] Oracle registry is paused; deferring vote submission for invoice ${invoiceId} until it resumes.`,
          );
          return;
        }
        console.log(
          `[Verifier] Could not open round for invoice ${invoiceId} (likely already open): ${openError}`,
        );
      }
    }

    let txHash: string;
    try {
      txHash = await retryWithBackoff(
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
    } catch (voteError) {
      if (isRegistryPausedError(voteError)) {
        this.consensusTracker?.markPaused();
        console.warn(
          `[Verifier] Oracle registry is paused; vote for invoice ${invoiceId} was not submitted and will be retried once it resumes.`,
        );
        return;
      }
      throw voteError;
    }
    console.log(`[Verifier] Vote (${approved}) submitted for invoice ${invoiceId}. Tx Hash: ${txHash}`);
  }

  private async verifyDocument(uri: string, expectedHash?: string): Promise<boolean> {
    if (!uri) {
      throw new Error('Document URI is empty');
    }

    // Fetch the document with a timeout and size limit
    const controller = new AbortController();
    const timeout = setTimeout(() => controller.abort(), 30_000);

    let response: Response;
    try {
      response = await fetch(uri, {
        signal: controller.signal,
        headers: { Accept: '*/*' },
      });
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      throw new Error(`Failed to fetch document from ${uri}: ${msg}`);
    } finally {
      clearTimeout(timeout);
    }

    if (!response.ok) {
      throw new Error(`Document fetch failed with HTTP ${response.status}: ${uri}`);
    }

    // Enforce a 10 MB size limit to prevent abuse
    const contentLength = response.headers.get('content-length');
    if (contentLength && parseInt(contentLength, 10) > 10 * 1024 * 1024) {
      throw new Error(`Document exceeds 10 MB size limit: ${uri}`);
    }

    const buffer = Buffer.from(await response.arrayBuffer());
    if (buffer.length === 0) {
      throw new Error(`Document is empty: ${uri}`);
    }

    // Compute SHA-256 hash of the document bytes
    const computedHash = createHash('sha256').update(buffer).digest('hex');

    // If no expected hash was provided, we cannot verify — reject to be safe
    if (!expectedHash) {
      throw new Error('No verification hash provided for document verification');
    }

    return computedHash === expectedHash;
  }
}
