import { Horizon, xdr, scValToNative } from '@stellar/stellar-sdk';
import { OracleConfig } from './types';
import { Verifier } from './verifier';

export class Listener {
  private config: OracleConfig;
  private verifier: Verifier;
  private horizon: Horizon.Server;

  constructor(config: OracleConfig, verifier: Verifier) {
    this.config = config;
    this.verifier = verifier;
    this.horizon = new Horizon.Server(config.horizonUrl);
  }

  async start() {
    console.log(`[Listener] Starting event listener...`);
    console.log(`[Listener] Horizon: ${this.config.horizonUrl}`);
    console.log(`[Listener] Contract ID: ${this.config.invoiceContractId}`);

    // Subscribe to contract effects (which include events)
    // Note: In a production environment, you should persist the cursor to resume after restart.
    this.horizon.effects()
      .forContract(this.config.invoiceContractId)
      .cursor('now')
      .stream({
        onmessage: (effect: any) => {
          this.handleEffect(effect);
        },
        onerror: (error: any) => {
          console.error('[Listener] Stream error:', error);
        }
      });
  }

  private handleEffect(effect: any) {
    // Check if it's a contract event
    if (effect.type !== 'contract_event' && effect.type !== 'contract') {
      return;
    }

    try {
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
