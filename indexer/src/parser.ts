/**
 * Parse Stellar Horizon events into structured Astera event records.
 */

/**
 * Logical category of the source contract for an indexed event. Used to
 * route credit_score events (#700) separately from invoice/pool events so
 * the REST API can filter by contract type.
 */
export type ContractType =
  | 'invoice'
  | 'pool'
  | 'credit_score'
  | 'oracle_registry'
  | 'compliance'
  | 'tranche'
  | 'insurance'
  | 'unknown';

export interface IndexedEvent {
  id: string;
  contractId: string;
  contractType: ContractType;
  eventType: string;
  topic: string[];
  value: any;
  actor: string | null;
  ledgerSequence: number;
  ledgerCloseAt: string;
  txHash: string;
  createdAt: string;
}

const CREDIT_SCORE_CONTRACT_ID = (process.env.CREDIT_SCORE_CONTRACT_ID || '').trim();
const INVOICE_CONTRACT_ID = (process.env.INVOICE_CONTRACT_ID || '').trim();
const POOL_CONTRACT_ID = (process.env.POOL_CONTRACT_ID || '').trim();
// #1044: secondary-market listings satellite contract, split out of pool to
// clear the 200KB wasm deploy limit. Emits lst_open/lst_cncl/lst_buy under
// its own "market" topic — see SECONDARY_MARKET_EVENT_TYPES below.
const SECONDARY_MARKET_CONTRACT_ID = (process.env.SECONDARY_MARKET_CONTRACT_ID || '').trim();
// #861: N-of-M staked oracle consensus network
const ORACLE_REGISTRY_CONTRACT_ID = (process.env.ORACLE_REGISTRY_CONTRACT_ID || '').trim();
// #867: on-chain compliance / sanctions screening registry
const COMPLIANCE_CONTRACT_ID = (process.env.COMPLIANCE_CONTRACT_ID || '').trim();
// #862: invoice tranching (senior/junior) with waterfall repayment and loss allocation
const TRANCHE_CONTRACT_ID = (process.env.TRANCHE_CONTRACT_ID || '').trim();
// #1036: collateral-liquidation Dutch auction + oracle-priced, multi-asset
// collateral risk-response satellite, split out of pool to clear the 200KB
// wasm deploy limit. Emits under its own "auction" topic — see
// AUCTION_EVENT_TYPES below.
const AUCTION_CONTRACT_ID = (process.env.AUCTION_CONTRACT_ID || '').trim();
// #1055: default-insurance reserve — coverage purchase, claims, reserve health
const INSURANCE_CONTRACT_ID = (process.env.INSURANCE_CONTRACT_ID || '').trim();

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

// #1025/#1035: secondary market events, now emitted by the secondary_market
// contract (see #1044) under its own "market" topic rather than pool's
// "pool" topic — the #1025 fixed-price listing lifecycle (lst_*) and the
// #1035 limit order book (ord_*) sit side by side in the same contract.
// Classified as 'pool' below to keep the existing API category consumers
// already filter on.
const SECONDARY_MARKET_EVENT_TYPES = new Set([
  'lst_open',
  'lst_cncl',
  'lst_buy',
  'ord_open',
  'ord_cncl',
  'ord_fill',
  'ord_exp',
]);

// #1036: auction contract events (Dutch collateral sale + risk-response
// monitoring), classified as 'pool' below — same satellite-of-pool rationale
// as secondary_market above.
const AUCTION_EVENT_TYPES = new Set([
  'col_risk',
  'col_safe',
  'auc_liq',
  'risk_cfg',
  'sale_open',
  'sale_take',
  'sale_exp',
]);

// #867: compliance contract emits under the "COMPLY" topic
const COMPLIANCE_EVENT_TYPES = new Set([
  'screened',
  'review',
  'scr_prop',
  'scr_reg',
  'scr_del',
  'scr_can',
  'int_set',
  'tl_set',
  'paused',
  'unpaused',
]);

// #862: tranche contract emits these event subtypes under the "TRANCHE" topic
const TRANCHE_EVENT_TYPES = new Set([
  'deposit',
  'withdraw',
  'fund',
  'repay',
  'default',
  'config',
]);

// #1055: insurance contract emits these event subtypes under the "INSURNCE" topic
const INSURANCE_EVENT_TYPES = new Set([
  'init',
  'cfg_set',
  'mcr_set',
  'funded',
  'paused',
  'unpaused',
  'covered',
  'claimed',
  'min_rsv',
]);

// #700: credit_score contract emits these event subtypes under the "CREDIT" topic
const CREDIT_SCORE_EVENT_TYPES = new Set([
  'payment',
  'default',
  'score_cfg',
  'thresh',
  'lt_upd',
  'hist_upd',
  // #868: external attestations + dispute mechanism
  'att_reg',
  'att_deact',
  'att_sub',
  'att_disp',
  'att_res',
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
  // #1044: secondary_market's listing events are still surfaced as 'pool'
  // category events — it's a satellite of pool, not a distinct product area.
  if (SECONDARY_MARKET_CONTRACT_ID && contractId === SECONDARY_MARKET_CONTRACT_ID) {
    return 'pool';
  }
  if (ORACLE_REGISTRY_CONTRACT_ID && contractId === ORACLE_REGISTRY_CONTRACT_ID) {
    return 'oracle_registry';
  }
  if (COMPLIANCE_CONTRACT_ID && contractId === COMPLIANCE_CONTRACT_ID) {
    return 'compliance';
  }
  if (TRANCHE_CONTRACT_ID && contractId === TRANCHE_CONTRACT_ID) {
    return 'tranche';
  }
  // #1036: auction's events are still surfaced as 'pool' category events —
  // it's a satellite of pool, not a distinct product area (same rationale
  // as secondary_market above).
  if (AUCTION_CONTRACT_ID && contractId === AUCTION_CONTRACT_ID) {
    return 'pool';
  }
  if (INSURANCE_CONTRACT_ID && contractId === INSURANCE_CONTRACT_ID) {
    return 'insurance';
  }
  // Fallback: infer from topic. credit_score events publish under "CREDIT",
  // oracle_registry events publish under "ORACLE" (#861),
  // compliance events publish under "COMPLY" (#867),
  // tranche events publish under "TRANCHE" (#862),
  // insurance events publish under "INSURNCE" (#1055).
  if (contractType === 'CREDIT' || CREDIT_SCORE_EVENT_TYPES.has(eventType)) {
    return 'credit_score';
  }
  if (contractType === 'ORACLE' || ORACLE_REGISTRY_EVENT_TYPES.has(eventType)) {
    return 'oracle_registry';
  }
  if (contractType === 'COMPLY' || COMPLIANCE_EVENT_TYPES.has(eventType)) {
    return 'compliance';
  }
  if (contractType === 'TRANCHE' || TRANCHE_EVENT_TYPES.has(eventType)) {
    return 'tranche';
  }
  if (contractType === 'INSURNCE' || INSURANCE_EVENT_TYPES.has(eventType)) {
    return 'insurance';
  }
  if (contractType === 'invoice') return 'invoice';
  if (contractType === 'pool') return 'pool';
  if (SECONDARY_MARKET_EVENT_TYPES.has(eventType)) return 'pool';
  if (contractType === 'auction' || AUCTION_EVENT_TYPES.has(eventType)) return 'pool';
  return 'unknown';
}

export function parseEvents(records: any[]): IndexedEvent[] {
  const events: IndexedEvent[] = [];

  for (const record of records) {
    try {
      if (record.type !== 'contract') continue;

      const topic = parseTopic(record);
      if (!topic) {
        console.warn('[parser] Skipping event record with missing topic:', record.id);
        continue;
      }

      const [contractType, eventType] = topic;
      const contractId = record.contract || '';

      const value = parseValue(record);
      events.push({
        id: record.id || `${record.paging_token}`,
        contractId,
        contractType: classifyContract(contractId, contractType, eventType || ''),
        eventType: eventType || 'unknown',
        topic: [contractType, eventType],
        value,
        actor: extractActor(contractType, eventType, value),
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
    const topic = record.topic || record.contract?.[0]?.topic;
    if (!topic || !Array.isArray(topic) || topic.length === 0) return null;
    if (topic.length === 1) return [String(topic[0]), 'generic'];
    return [String(topic[0]), String(topic[1])];
  } catch {
    return null;
  }
}

function parseValue(record: any): any {
  try {
    return record.value ?? record.contract?.[0]?.value ?? null;
  } catch {
    return null;
  }
}

function isStellarAddress(val: any): boolean {
  return typeof val === 'string' && /^(G[A-Z2-7]{55}|C[A-Z2-7]{55})$/.test(val);
}

/**
 * Extract the actor/caller address from event value based on event type.
 * Resilient to schema changes across contract migrations (handles tuples, objects, and scalars).
 */
function extractActor(contractType: string, eventType: string, value: any): string | null {
  if (!value) return null;

  if (isStellarAddress(value)) return value;

  // Object schema (struct-emitted or post-migration events)
  if (typeof value === 'object' && !Array.isArray(value)) {
    const candidates = [
      value.actor,
      value.owner,
      value.caller,
      value.investor,
      value.sme,
      value.user,
      value.sender,
      value.screener,
    ];
    for (const cand of candidates) {
      if (isStellarAddress(cand)) return cand;
    }
    for (const val of Object.values(value)) {
      if (isStellarAddress(val)) return val as string;
    }
  }

  // Tuple array schema (pre and post-migration)
  if (Array.isArray(value) && value.length > 0) {
    if (contractType === 'POOL') {
      switch (eventType) {
        case 'deposit':
        case 'withdraw':
        case 'repaid':
        case 'part_pay':
        case 'yld_claim':
        case 'wd_full':
        case 'wd_queue':
        case 'wd_cncl':
          if (isStellarAddress(value[0])) return value[0];
          break;
        // #1036: (invoice_id, depositor, token, amount, ...) — depositor is value[1]
        // (col_dep used to also carry a second, duplicate depositor at the
        // end of the tuple; this case previously — incorrectly — checked
        // value[0], which is invoice_id here, not an address)
        case 'col_dep':
        case 'col_topup':
        // (invoice_id, depositor, amount) — published by pool's trusted
        // risk_liquidate_collateral, called by the auction risk-response
        // satellite (matches execute_seize_collateral's own seizure event
        // shape, which also doesn't carry token)
        case 'col_liq':
          if (isStellarAddress(value[1])) return value[1];
          break;
        // #1036: (admin, risk_contract) — admin is value[0]
        case 'set_risk':
          if (isStellarAddress(value[0])) return value[0];
          break;
        // #1025: secondary market — seller is value[2], buyer is value[3]
        case 'lst_open':
        case 'lst_cncl':
          if (isStellarAddress(value[2])) return value[2];
          break;
        case 'lst_buy':
          if (isStellarAddress(value[3])) return value[3];
          break;
      }
    } else if (contractType === 'auction') {
      // #1036: auction (collateral-liquidation Dutch sale + risk-response
      // monitoring) publishes under its own lowercase "auction" topic — see
      // `EVT` in contracts/auction/src/lib.rs.
      switch (eventType) {
        case 'sale_open':
        case 'sale_take':
        case 'sale_exp':
        case 'auc_liq':
          if (isStellarAddress(value[1])) return value[1];
          break;
        case 'risk_cfg':
          if (isStellarAddress(value[0])) return value[0];
          break;
        // (invoice_id, ratio_bps) — system-triggered by a permissionless
        // keeper call, no actor address in the payload
        case 'col_risk':
        case 'col_safe':
          return null;
      }
    } else if (contractType === 'market') {
      // #1035: secondary_market publishes its own events under a "market"
      // topic (not "POOL") — ord_open/ord_cncl/ord_exp carry the order
      // owner at value[2] (order_id, invoice_id, owner, ...), and ord_fill
      // carries both sides of the trade, with the buyer first at value[3]
      // (taker_order_id, maker_order_id, invoice_id, buyer, seller, ...).
      switch (eventType) {
        case 'ord_open':
        case 'ord_cncl':
        case 'ord_exp':
          if (isStellarAddress(value[2])) return value[2];
          break;
        case 'ord_fill':
          if (isStellarAddress(value[3])) return value[3];
          break;
      }
    } else if (contractType === 'INVOICE') {
      switch (eventType) {
        case 'created':
          if (isStellarAddress(value[1])) return value[1];
          if (isStellarAddress(value[0])) return value[0];
          break;
        case 'funded':
        case 'paid':
        case 'cancelled':
        case 'resolved':
          if (isStellarAddress(value[1])) return value[1];
          if (isStellarAddress(value[0])) return value[0];
          break;
        case 'paused':
        case 'unpaused':
          if (isStellarAddress(value[0])) return value[0];
          break;
      }
    } else if (contractType === 'CREDIT') {
      switch (eventType) {
        case 'payment':
        case 'default':
          if (isStellarAddress(value[0])) return value[0];
          break;
      }
    } else if (contractType === 'INSURNCE') {
      // #1055: insurance publishes under its own "INSURNCE" topic — see `EVT`
      // in contracts/insurance/src/lib.rs.
      switch (eventType) {
        // (invoice_id, payer, premium, coverage_bps) — payer is value[1]
        case 'covered':
          if (isStellarAddress(value[1])) return value[1];
          break;
        // (admin, token, amount|min_ratio_bps|min_amount) — admin is value[0]
        case 'funded':
        case 'mcr_set':
        case 'min_rsv':
          if (isStellarAddress(value[0])) return value[0];
          break;
        // (invoice_id, payout) — no actor address in the payload
        case 'claimed':
          return null;
      }
    }

    // Fallback: scan array elements for a valid Stellar address
    for (const item of value) {
      if (isStellarAddress(item)) return item;
    }
  }

  // Tranche events: first field is usually the actor
  if (contractType === 'TRANCHE') {
    switch (eventType) {
      case 'deposit':
      case 'withdraw':
      case 'fund':
      case 'repay':
      case 'default':
        return value[0]; // investor or caller
      case 'config':
        return value[0]; // admin
    }
  }

  return null;
}
