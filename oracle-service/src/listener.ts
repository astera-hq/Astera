import * as fs from 'fs';
import { Horizon, xdr, scValToNative } from '@stellar/stellar-sdk';
import { OracleConfig } from './types';
import { Verifier } from './verifier';
import { ConsensusTracker } from './consensus';
import { StakingMetricsTracker } from './staking';

export class Listener {
  private config: OracleConfig;
  private verifier: Verifier;
  private horizon: Horizon.Server;
  private consensusTracker?: ConsensusTracker;
  private stakingTracker?: StakingMetricsTracker;
  public processedCount = 0;

  constructor(
    config: OracleConfig,
    verifier: Verifier,
    consensusTracker?: ConsensusTracker,
    stakingTracker?: StakingMetricsTracker,
  ) {
    this.config = config;
    this.verifier = verifier;
    this.consensusTracker = consensusTracker;
    this.stakingTracker = stakingTracker;
    this.horizon = new Horizon.Server(config.horizonUrl);
  }

  async start() {
    console.log(`[Listener] Starting event listener...`);
    console.log(`[Listener] Horizon: ${this.config.horizonUrl}`);
    console.log(`[Listener] Invoice contract: ${this.config.invoiceContractId}`);
    if (this.config.oracleRegistryContractId) {
      console.log(`[Listener] Oracle registry contract: ${this.config.oracleRegistryContractId}`);
    }

    // #980: resume from the last successfully processed ledger position
    // instead of always starting from 'now', so a restart neither misses
    // events that occurred while the service was down nor resubmits
    // already-processed votes by guessing at a lookback window.
    const cursor = this.loadCursor();
    console.log(`[Listener] Starting stream from cursor: ${cursor}`);

    this.horizon.effects()
      .cursor(cursor)
      .stream({
        onmessage: (effect: any) => {
          this.handleEffect(effect);
          if (effect.paging_token) {
            this.saveCursor(effect.paging_token);
          }
        },
        onerror: (error: any) => {
          console.error('[Listener] Stream error:', error);
        }
      });
  }

  private loadCursor(): string {
    try {
      const saved = fs.readFileSync(this.config.listenerCursorPath, 'utf-8').trim();
      if (saved) {
        return saved;
      }
    } catch {
      // No checkpoint file yet (first run, or it was deleted) — start fresh.
    }
    return 'now';
  }

  private saveCursor(pagingToken: string): void {
    try {
      fs.writeFileSync(this.config.listenerCursorPath, pagingToken, 'utf-8');
    } catch (error) {
      console.error('[Listener] Failed to persist stream cursor:', error);
    }
  }

  private handleEffect(effect: any) {
    // Check if it's a contract event
    if (effect.type !== 'contract_event' && effect.type !== 'contract') {
      return;
    }

    try {
      const isInvoiceContract = effect.contract_id === this.config.invoiceContractId;
      const isRegistryContract =
        !!this.config.oracleRegistryContractId &&
        effect.contract_id === this.config.oracleRegistryContractId;

      // Filter by contract ID if present on the effect — accept either the
      // invoice contract (legacy `created` events) or, if configured, the
      // #861 oracle registry (round-status events for `ConsensusTracker`).
      if (effect.contract_id && !isInvoiceContract && !isRegistryContract) {
        return;
      }

      this.processedCount += 1;

      // Horizon effects for Soroban events typically have the topic and value
      // This part depends on how Horizon represents contract events in effects.
      // Based on the indexer implementation:
      const topicXdr = effect.topic;
      const valueXdr = effect.value;

      if (!topicXdr || !Array.isArray(topicXdr) || topicXdr.length < 2) {
        return;
      }

      // Topics are usually base64-encoded ScVal XDR
      const segment1 = this.decodeScVal(topicXdr[0]);
      const segment2 = this.decodeScVal(topicXdr[1]);

      console.log(`[Listener] Detected event: [${segment1}, ${segment2}]`);

      if (segment1 === 'INVOICE' && segment2 === 'created') {
        const value = this.decodeScVal(valueXdr);
        // The 'created' event payload is (id, owner, amount, metadata_uri)
        if (Array.isArray(value)) {
          const invoiceId = BigInt(value[0]);
          console.log(`[Listener] New invoice detected! ID: ${invoiceId}`);
          this.verifier.verifyInvoice(invoiceId);
        }
        return;
      }

      // #861: forward every event under the registry's "ORACLE" topic
      // namespace to the consensus tracker so the health endpoint can report
      // live round state without a separate polling loop.
      if (segment1 === 'ORACLE') {
        const value = this.decodeScVal(valueXdr);
        this.consensusTracker?.handleEvent(segment2, value);
        // #979: also forward to the staking metrics tracker, which filters
        // for `slashed` events concerning this node's own oracle address.
        this.stakingTracker?.handleEvent(segment2, value, effect.created_at);
      }
    } catch (error) {
      console.error('[Listener] Failed to process effect:', error);
    }
  }

  private decodeScVal(base64Xdr: string): any {
    try {
      const val = xdr.ScVal.fromXDR(base64Xdr, 'base64');
      return scValToNative(val);
    } catch {
      return null;
    }
  }
}
