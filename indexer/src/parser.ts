/**
 * Parse Stellar Horizon events into structured Astera event records.
 */

/**
 * Logical category of the source contract for an indexed event. Used to
 * route credit_score events (#700) separately from invoice/pool events so
 * the REST API can filter by contract type.
 */
export type ContractType = 'invoice' | 'pool' | 'credit_score' | 'oracle_registry' | 'unknown';

export interface IndexedEvent {
  id: string;
  contractId: string;
  contractType: ContractType;
  eventType: string;
  topic: string[];
  value: any;
  ledgerSequence: number;
  ledgerCloseAt: string;
  txHash: string;
  createdAt: string;
}

const CREDIT_SCORE_CONTRACT_ID = (process.env.CREDIT_SCORE_CONTRACT_ID || '').trim();
const INVOICE_CONTRACT_ID = (process.env.INVOICE_CONTRACT_ID || '').trim();
const POOL_CONTRACT_ID = (process.env.POOL_CONTRACT_ID || '').trim();
// #861: N-of-M staked oracle consensus network
const ORACLE_REGISTRY_CONTRACT_ID = (process.env.ORACLE_REGISTRY_CONTRACT_ID || '').trim();

// #861: oracle_registry contract emits these event subtypes under the
// "ORACLE" topic (see `EVT` in contracts/oracle_registry/src/lib.rs).
const ORACLE_REGISTRY_EVENT_TYPES = new Set([
  'registrd',
  'dreg_req',
  'dreg_done',
  'slashed',
  'rnd_open',
  'voted',
  'consensus',
  'rnd_exp',
  'fallback',
  'inv_set',
  'cfg_upd',
  'paused',
  'unpaused',
]);

// #700: credit_score contract emits these event subtypes under the "CREDIT" topic
const CREDIT_SCORE_EVENT_TYPES = new Set([
  'payment',
  'default',
  'score_cfg',
  'thresh',
  'lt_upd',
  'hist_upd',
]);

function classifyContract(contractId: string, contractType: string, eventType: string): ContractType {
  if (CREDIT_SCORE_CONTRACT_ID && contractId === CREDIT_SCORE_CONTRACT_ID) {
    return 'credit_score';
  }
  if (INVOICE_CONTRACT_ID && contractId === INVOICE_CONTRACT_ID) {
    return 'invoice';
  }
  if (POOL_CONTRACT_ID && contractId === POOL_CONTRACT_ID) {
    return 'pool';
  }
  if (ORACLE_REGISTRY_CONTRACT_ID && contractId === ORACLE_REGISTRY_CONTRACT_ID) {
    return 'oracle_registry';
  }
  // Fallback: infer from topic. credit_score events publish under "CREDIT",
  // oracle_registry events publish under "ORACLE" (#861).
  if (contractType === 'CREDIT' || CREDIT_SCORE_EVENT_TYPES.has(eventType)) {
    return 'credit_score';
  }
  if (contractType === 'ORACLE' || ORACLE_REGISTRY_EVENT_TYPES.has(eventType)) {
    return 'oracle_registry';
  }
  if (contractType === 'invoice') return 'invoice';
  if (contractType === 'pool') return 'pool';
  return 'unknown';
}

export function parseEvents(records: any[]): IndexedEvent[] {
  const events: IndexedEvent[] = [];

  for (const record of records) {
    try {
      if (record.type !== 'contract') continue;

      const topic = parseTopic(record);
      if (!topic) continue;

      const [contractType, eventType] = topic;
      const contractId = record.contract || '';

      events.push({
        id: record.id || `${record.paging_token}`,
        contractId,
        contractType: classifyContract(contractId, contractType, eventType || ''),
        eventType: eventType || 'unknown',
        topic: [contractType, eventType],
        value: parseValue(record),
        ledgerSequence: record.ledger_sequence || 0,
        ledgerCloseAt: record.created_at || new Date().toISOString(),
        txHash: record.transaction_hash || '',
        createdAt: new Date().toISOString(),
      });
    } catch (err) {
      console.error('[parser] Failed to parse event:', err);
    }
  }

  return events;
}

function parseTopic(record: any): [string, string] | null {
  try {
    const topic = record.contract?.[0]?.topic;
    if (!topic || !Array.isArray(topic) || topic.length < 2) return null;
    // Topics are base64-encoded xdr.ScVal
    // For simplicity, we expect the topic to be an array of strings
    return [topic[0], topic[1]];
  } catch {
    return null;
  }
}

function parseValue(record: any): any {
  try {
    return record.contract?.[0]?.value || null;
  } catch {
    return null;
  }
}
