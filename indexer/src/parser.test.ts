import assert from 'node:assert';
import { parseEvents } from './parser';

const TEST_ADDRESS = 'GBRPYHIL2CI3FNQ4BXLFMNDLFJUNPU2HY3ZMFXYCZTM6WPIXY6OROLET';

function runTests() {
  console.log('[indexer parser test] Running tests...');

  // Test 1: single-topic migration event
  const records1 = [
    {
      type: 'contract',
      id: '000000100',
      ledger_sequence: 12345,
      created_at: '2026-07-26T20:00:00Z',
      transaction_hash: 'txhash123',
      contract: [
        {
          topic: ['migrated'],
          value: [TEST_ADDRESS, 'v2.0.0'],
        },
      ],
    },
  ];

  const events1 = parseEvents(records1);
  assert.strictEqual(events1.length, 1);
  assert.deepStrictEqual(events1[0].topic, ['migrated', 'generic']);
  assert.strictEqual(events1[0].eventType, 'generic');
  assert.strictEqual(events1[0].actor, TEST_ADDRESS);

  // Test 2: object-shaped event emitted post-migration
  const records2 = [
    {
      type: 'contract',
      id: '000000101',
      ledger_sequence: 12346,
      created_at: '2026-07-26T20:01:00Z',
      transaction_hash: 'txhash456',
      contract: [
        {
          topic: ['INVOICE', 'created'],
          value: {
            id: 42,
            owner: TEST_ADDRESS,
            amount: '5000000000',
          },
        },
      ],
    },
  ];

  const events2 = parseEvents(records2);
  assert.strictEqual(events2.length, 1);
  assert.strictEqual(events2[0].eventType, 'created');
  assert.strictEqual(events2[0].actor, TEST_ADDRESS);

  // Test 3: tuple-shaped legacy event emitted pre-migration
  const records3 = [
    {
      type: 'contract',
      id: '000000102',
      ledger_sequence: 12347,
      created_at: '2026-07-26T20:02:00Z',
      transaction_hash: 'txhash789',
      contract: [
        {
          topic: ['POOL', 'deposit'],
          value: [TEST_ADDRESS, '1000000000', '1000', '1700000000'],
        },
      ],
    },
  ];

  const events3 = parseEvents(records3);
  assert.strictEqual(events3.length, 1);
  assert.strictEqual(events3[0].eventType, 'deposit');
  assert.strictEqual(events3[0].actor, TEST_ADDRESS);

  // Test 4 (#1044): secondary_market's lst_open/lst_cncl/lst_buy events
  // publish under their own "market" topic (not pool's "pool"/"POOL"), and
  // SECONDARY_MARKET_CONTRACT_ID isn't set in this test process — this
  // exercises the SECONDARY_MARKET_EVENT_TYPES fallback in classifyContract
  // that routes them to the 'pool' category by event type alone. Value
  // shape matches contracts/secondary_market/src/lib.rs's lst_open publish
  // call: (listing_id, invoice_id, seller, amount_or_bps, price).
  const records4 = [
    {
      type: 'contract',
      id: '000000103',
      ledger_sequence: 12348,
      created_at: '2026-07-26T20:03:00Z',
      transaction_hash: 'txhash999',
      contract: [
        {
          topic: ['market', 'lst_open'],
          value: [7, 42, TEST_ADDRESS, '1000', '1500'],
        },
      ],
    },
  ];

  const events4 = parseEvents(records4);
  assert.strictEqual(events4.length, 1);
  assert.strictEqual(events4[0].contractType, 'pool');
  assert.strictEqual(events4[0].eventType, 'lst_open');
  assert.strictEqual(events4[0].actor, TEST_ADDRESS);

  // Test 5 (#1044): lst_cncl and lst_buy route the same way.
  const records5 = [
    {
      type: 'contract',
      id: '000000104',
      ledger_sequence: 12349,
      created_at: '2026-07-26T20:04:00Z',
      transaction_hash: 'txhashaaa',
      contract: [{ topic: ['market', 'lst_cncl'], value: [7, 42, TEST_ADDRESS] }],
    },
    {
      type: 'contract',
      id: '000000105',
      ledger_sequence: 12350,
      created_at: '2026-07-26T20:05:00Z',
      transaction_hash: 'txhashbbb',
      contract: [{ topic: ['market', 'lst_buy'], value: [7, 42, TEST_ADDRESS, TEST_ADDRESS, '1500'] }],
    },
  ];

  const events5 = parseEvents(records5);
  assert.strictEqual(events5.length, 2);
  assert.strictEqual(events5[0].contractType, 'pool');
  assert.strictEqual(events5[1].contractType, 'pool');

  console.log('[indexer parser test] All parser tests passed!');
}

runTests();
