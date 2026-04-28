/**
 * Parse Stellar Horizon events into structured Astera event records.
 */

export interface IndexedEvent {
  id: string;
  contractId: string;
  eventType: string;
  topic: string[];
  value: any;
  ledgerSequence: number;
  ledgerCloseAt: string;
  txHash: string;
  createdAt: string;
}

export function parseEvents(records: any[]): IndexedEvent[] {
  const events: IndexedEvent[] = [];

  for (const record of records) {
    try {
      if (record.type !== 'contract') continue;

      const topic = parseTopic(record);
      if (!topic) continue;

      const [contractType, eventType] = topic;

      events.push({
        id: record.id || `${record.paging_token}`,
        contractId: record.contract || '',
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
