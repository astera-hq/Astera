import type { Page } from '@playwright/test';
import { nativeToScVal, xdr } from '@stellar/stellar-sdk';
const INVOICE_CONTRACT_ID =
  process.env.NEXT_PUBLIC_INVOICE_CONTRACT_ID ??
  'CInvoiceAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM';

function extractContractMethod(txXdr: string): string | null {
  try {
    const envelope = xdr.TransactionEnvelope.fromXDR(txXdr, 'base64');
    const v1Envelope = envelope.v1();
    if (!v1Envelope) return null;

    const tx = v1Envelope.tx();
    const ops = tx.operations();
    if (ops.length === 0) return null;

    const firstOp = ops.at(0);
    if (!firstOp) return null;

    const body = firstOp.body();
    if (body.switch().name !== 'invokeHostFunction') return null;

    const hostFn = body.invokeHostFunctionOp().hostFunction();
    if (hostFn.switch().name !== 'hostFunctionTypeInvokeContract') return null;

    return hostFn.invokeContract().functionName().toString();
  } catch {
    return null;
  }
}

function buildPoolConfig(adminAddress: string) {
  return {
    invoice_contract: INVOICE_CONTRACT_ID,
    admin: adminAddress,
    yield_bps: 800,
    factoring_fee_bps: 150,
    compound_interest: false,
    proposed_yield_bps: 0,
    yield_proposal_at: 0,
    yield_timelock_secs: 0,
    max_single_investor_bps: 5000,
    max_withdrawal_queue_age_days: 7,
  };
}

// #861: N-of-M staked oracle consensus network
function buildOracleRegistryConfig(): Record<string, unknown> {
  return {
    min_stake: '1000000',
    stake_token: INVOICE_CONTRACT_ID,
    required_votes: 3,
    quorum_bps: 6600,
    round_duration_secs: 259200,
    deregister_cooldown_secs: 604800,
    treasury: null,
  };
}

function buildOracleInfo(address: string): Record<string, unknown> {
  return {
    address,
    stake_amount: '5000000',
    stake_token: INVOICE_CONTRACT_ID,
    is_active: true,
    total_verifications: 12,
    total_slashes: 0,
    registered_at: Math.floor(Date.now() / 1000) - 86400,
    deregister_requested_at: null,
  };
}

function mockReturnValue(method: string | null, adminAddress: string): xdr.ScVal {
  switch (method) {
    case 'get_config':
      return nativeToScVal(buildPoolConfig(adminAddress));
    case 'get_invoice_count':
      return nativeToScVal(0, { type: 'u64' });
    case 'get_multiple_invoices':
      return nativeToScVal([]);
    case 'is_paused':
      return nativeToScVal(false);
    case 'get_registry_config':
      return nativeToScVal(buildOracleRegistryConfig());
    case 'list_active_oracles':
      return nativeToScVal([adminAddress, INVOICE_CONTRACT_ID]);
    case 'get_oracle_info':
      return nativeToScVal(buildOracleInfo(adminAddress));
    case 'get_verification_round':
      return nativeToScVal(null);
    default:
      return nativeToScVal(null);
  }
}

function buildSimulateSuccess(id: number | string | undefined, retval: xdr.ScVal) {
  return {
    jsonrpc: '2.0',
    id,
    result: {
      transactionData:
        'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==',
      minResourceFee: '100',
      events: [],
      results: [],
      cost: {
        cpuInsns: '0',
        memBytes: '0',
      },
      latestLedger: 1,
      retval: retval.toXDR('base64'),
    },
  };
}

/** Stub Soroban RPC calls needed for the admin dashboard and admin guard. */
export async function stubAdminDashboardContracts(page: Page, adminAddress: string): Promise<void> {
  await page.route('**/*stellar.org/**', async (route) => {
    const request = route.request();
    if (request.method() !== 'POST') {
      await route.continue();
      return;
    }

    const body = request.postDataJSON() as {
      id?: number | string;
      method?: string;
      params?: { transaction?: string; address?: string };
    };

    if (body.method === 'getAccount') {
      await route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({
          jsonrpc: '2.0',
          id: body.id,
          result: {
            sequence: '1',
            sequenceLedger: 1,
            sequenceTime: Math.floor(Date.now() / 1000),
          },
        }),
      });
      return;
    }

    if (body.method === 'simulateTransaction' && body.params?.transaction) {
      const method = extractContractMethod(body.params.transaction);
      const retval = mockReturnValue(method, adminAddress);
      await route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify(buildSimulateSuccess(body.id, retval)),
      });
      return;
    }

    if (body.method === 'getLatestLedger') {
      await route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({
          jsonrpc: '2.0',
          id: body.id,
          result: { sequence: 1, protocolVersion: 22, id: '1' },
        }),
      });
      return;
    }

    await route.continue();
  });
}
