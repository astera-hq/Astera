import type { Page } from '@playwright/test';
import { nativeToScVal, scValToNative, xdr } from '@stellar/stellar-sdk';

const POOL_CONTRACT_ID =
  process.env.NEXT_PUBLIC_POOL_CONTRACT_ID ??
  'CAAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQCAIBAEAQC526';

interface ProposalRecord {
  role: string;
  target: string;
  // Flat `[tag, ...fields]`, matching how the real access_control contract
  // encodes an `ActionPayload` enum variant on the wire.
  action: unknown[];
  proposer: string;
  approvals: string[];
  created_at: number;
  expires_at: number;
  status: 'Pending' | 'Approved' | 'Executed' | 'Rejected';
}

export interface AccessControlStore {
  role: string;
  signers: string[];
  threshold: number;
  proposals: ProposalRecord[];
  poolYieldBps: number;
}

export function createAccessControlStore(
  role: string,
  signers: string[],
  threshold: number,
): AccessControlStore {
  return { role, signers, threshold, proposals: [], poolYieldBps: 800 };
}

function extractInvocation(txXdr: string): { method: string; args: xdr.ScVal[] } | null {
  try {
    const envelope = xdr.TransactionEnvelope.fromXDR(txXdr, 'base64');
    const v1Envelope = envelope.v1();
    if (!v1Envelope) return null;
    const ops = v1Envelope.tx().operations();
    const firstOp = ops.at(0);
    if (!firstOp) return null;
    const body = firstOp.body();
    if (body.switch().name !== 'invokeHostFunction') return null;
    const hostFn = body.invokeHostFunctionOp().hostFunction();
    if (hostFn.switch().name !== 'hostFunctionTypeInvokeContract') return null;
    const invoke = hostFn.invokeContract();
    return { method: invoke.functionName().toString(), args: invoke.args() };
  } catch {
    return null;
  }
}

function recomputeStatus(p: ProposalRecord, threshold: number) {
  if (p.status === 'Executed' || p.status === 'Rejected') return;
  p.status = p.approvals.length >= threshold ? 'Approved' : 'Pending';
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
      cost: { cpuInsns: '0', memBytes: '0' },
      latestLedger: 1,
      retval: retval.toXDR('base64'),
    },
  };
}

function buildTransactionResultXdr(): string {
  const result = new xdr.TransactionResult({
    feeCharged: new xdr.Int64(0),
    result: xdr.TransactionResultResult.txSuccess([]),
    ext: new xdr.TransactionResultExt(0),
  });
  return result.toXDR('base64');
}

function buildTransactionMetaXdr(): string {
  return new xdr.TransactionMeta(0, []).toXDR('base64');
}

// `Server.getAccount` (used to fetch the source account for every tx we
// build) doesn't call the simple `getAccount` JSON-RPC method — it calls
// `getLedgerEntries` for an `account` ledger key and reads `seqNum()` off the
// decoded entry. The account id embedded in the entry itself is never
// inspected by the caller (it re-uses the address it was asked for), so one
// fixed, validly-shaped `AccountEntry` blob works for every request.
function buildAccountEntryXdr(): string {
  const entry = new xdr.AccountEntry({
    accountId: xdr.PublicKey.publicKeyTypeEd25519(Buffer.alloc(32)),
    balance: xdr.Int64.fromString('1000000000000'),
    seqNum: xdr.Int64.fromString('1'),
    numSubEntries: 0,
    inflationDest: null,
    flags: 0,
    homeDomain: '',
    thresholds: Buffer.from([0, 0, 0, 0]),
    signers: [],
    ext: new xdr.AccountEntryExt(0),
  });
  return xdr.LedgerEntryData.account(entry).toXDR('base64');
}

let hashCounter = 0;
const pendingEnvelopes = new Map<string, string>();

/**
 * Stubs the Soroban RPC calls the access-control admin page and its
 * propose/approve/reject/revoke/execute transaction builders need, backed by
 * an in-memory `AccessControlStore` mutated as each signed transaction lands
 * — so two browser contexts sharing the same store (one per wallet) can
 * drive a real propose → approve → execute flow against the mock.
 */
export async function stubAccessControlContracts(
  page: Page,
  store: AccessControlStore,
  viewerAddress: string,
): Promise<void> {
  await page.route('**/*stellar.org/**', async (route) => {
    const request = route.request();
    if (request.method() !== 'POST') {
      await route.continue();
      return;
    }

    const body = request.postDataJSON() as {
      id?: number | string;
      method?: string;
      params?: { transaction?: string; keys?: string[] };
    };

    if (body.method === 'getLedgerEntries' && body.params?.keys) {
      const accountEntryXdr = buildAccountEntryXdr();
      await route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({
          jsonrpc: '2.0',
          id: body.id,
          result: {
            latestLedger: 1,
            entries: body.params.keys.map((key) => ({
              key,
              xdr: accountEntryXdr,
              lastModifiedLedgerSeq: 1,
            })),
          },
        }),
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

    if (body.method === 'simulateTransaction' && body.params?.transaction) {
      const inv = extractInvocation(body.params.transaction);
      const method = inv?.method ?? null;
      let retval: xdr.ScVal = nativeToScVal(null);

      if (method === 'get_role_config') {
        const role = scValToNative(inv!.args[0]!);
        const roleTag = Array.isArray(role) ? role[0] : role;
        retval =
          roleTag === store.role
            ? nativeToScVal({ signers: store.signers, threshold: store.threshold })
            : nativeToScVal(null);
      } else if (method === 'is_signer') {
        const role = scValToNative(inv!.args[0]!);
        const roleTag = Array.isArray(role) ? role[0] : role;
        const address = scValToNative(inv!.args[1]!) as string;
        retval = nativeToScVal(roleTag === store.role && store.signers.includes(address));
      } else if (method === 'get_next_proposal_id') {
        retval = nativeToScVal(store.proposals.length, { type: 'u64' });
      } else if (method === 'get_proposal') {
        const id = Number(scValToNative(inv!.args[0]!));
        const p = store.proposals[id];
        retval = p ? nativeToScVal({ ...p }) : nativeToScVal(null);
      } else if (method === 'get_config') {
        // The shared `/admin` layout's route guard only allows the wallet
        // matching this `admin` field through. `get_config` is always
        // simulated from a fixed read-only placeholder source account (see
        // `getPoolConfig` in lib/contracts.ts), not the connected wallet, so
        // we can't derive "who's asking" from the tx itself — each stubbed
        // page instead gets told which wallet is viewing it. (Not a
        // simulation of real access control; that legacy single-admin gate
        // is orthogonal to the #864 RBAC flow this mock exists to exercise.)
        retval = nativeToScVal({
          invoice_contract: POOL_CONTRACT_ID,
          admin: viewerAddress,
          yield_bps: store.poolYieldBps,
          factoring_fee_bps: 150,
          compound_interest: false,
          proposed_yield_bps: 0,
          yield_proposal_at: 0,
          yield_timelock_secs: 0,
          max_single_investor_bps: 5000,
          max_withdrawal_queue_age_days: 7,
          max_withdrawal_queue_depth: 500,
        });
      }
      // propose_action / approve_action / reject_action / revoke_approval /
      // execute_action don't need a meaningful simulated retval — the tx
      // builders only use the simulate response to assemble resource fees.

      await route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify(buildSimulateSuccess(body.id, retval)),
      });
      return;
    }

    if (body.method === 'sendTransaction' && body.params?.transaction) {
      const inv = extractInvocation(body.params.transaction);
      if (inv) {
        const { method, args } = inv;
        if (method === 'propose_action') {
          const proposer = scValToNative(args[1]!) as string;
          const target = scValToNative(args[2]!) as string;
          const action = scValToNative(args[3]!) as unknown[];
          const record: ProposalRecord = {
            role: store.role,
            target,
            action,
            proposer,
            approvals: [proposer],
            created_at: Math.floor(Date.now() / 1000),
            expires_at: Math.floor(Date.now() / 1000) + 86_400,
            status: 'Pending',
          };
          recomputeStatus(record, store.threshold);
          store.proposals.push(record);
        } else if (method === 'approve_action') {
          const signer = scValToNative(args[0]!) as string;
          const id = Number(scValToNative(args[1]!));
          const p = store.proposals[id];
          if (p && !p.approvals.includes(signer)) {
            p.approvals.push(signer);
            recomputeStatus(p, store.threshold);
          }
        } else if (method === 'reject_action') {
          const id = Number(scValToNative(args[1]!));
          const p = store.proposals[id];
          if (p) p.status = 'Rejected';
        } else if (method === 'revoke_approval') {
          const signer = scValToNative(args[0]!) as string;
          const id = Number(scValToNative(args[1]!));
          const p = store.proposals[id];
          if (p) {
            p.approvals = p.approvals.filter((a) => a !== signer);
            recomputeStatus(p, store.threshold);
          }
        } else if (method === 'execute_action') {
          const id = Number(scValToNative(args[1]!));
          const p = store.proposals[id];
          if (p && p.status === 'Approved') {
            p.status = 'Executed';
            const [tag, ...values] = p.action;
            if (tag === 'SetYield') store.poolYieldBps = values[0] as number;
          }
        }
      }

      hashCounter += 1;
      const hash = `mockhash${hashCounter}`.padEnd(64, '0');
      pendingEnvelopes.set(hash, body.params.transaction);
      await route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({
          jsonrpc: '2.0',
          id: body.id,
          result: {
            status: 'PENDING',
            hash,
            latestLedger: 1,
            latestLedgerCloseTime: Math.floor(Date.now() / 1000),
          },
        }),
      });
      return;
    }

    if (body.method === 'getTransaction') {
      const hash = (body.params as unknown as { hash?: string })?.hash ?? '';
      const envelopeXdr = pendingEnvelopes.get(hash) ?? '';
      await route.fulfill({
        contentType: 'application/json',
        body: JSON.stringify({
          jsonrpc: '2.0',
          id: body.id,
          result: {
            status: 'SUCCESS',
            latestLedger: 2,
            latestLedgerCloseTime: Math.floor(Date.now() / 1000),
            oldestLedger: 1,
            oldestLedgerCloseTime: Math.floor(Date.now() / 1000) - 100,
            txHash: hash,
            applicationOrder: 1,
            feeBump: false,
            envelopeXdr,
            resultXdr: buildTransactionResultXdr(),
            resultMetaXdr: buildTransactionMetaXdr(),
          },
        }),
      });
      return;
    }

    await route.continue();
  });
}
