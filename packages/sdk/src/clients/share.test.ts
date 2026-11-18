import { xdr, Address, Keypair, scValToNative } from '@stellar/stellar-sdk';
import { ShareClient } from './share';
import { Errors as ShareErrors } from '../generated/share';

// #1557: ShareClient wraps contracts/share/src/lib.rs. These tests pin the
// method-name / arg-encoding contract and response decoding without a live
// RPC endpoint.
const ADMIN = Keypair.random().publicKey();
const ALICE = Keypair.random().publicKey();
const BOB = Keypair.random().publicKey();

function makeClient(): ShareClient {
  return new ShareClient({
    rpcUrl: 'https://example.com',
    network: 'Test SDF Network ; September 2015',
    contractId: 'CSHARE',
  });
}

function i128(value: bigint): xdr.ScVal {
  return xdr.ScVal.scvI128(
    new xdr.Int128Parts({
      hi: xdr.Int64.fromString((value >> 64n).toString()),
      lo: xdr.Uint64.fromString((value & ((1n << 64n) - 1n)).toString()),
    }),
  );
}

function mockSimulate(client: ShareClient, retval: xdr.ScVal): jest.SpyInstance {
  return jest.spyOn(client as any, 'simulate').mockResolvedValue({ result: { retval } });
}

function mockSend(client: ShareClient): jest.SpyInstance {
  return jest.spyOn(client as any, 'buildAndSendTx').mockResolvedValue('txhash');
}

describe('ShareClient reads', () => {
  it('balance decodes an i128 to bigint and encodes the address', async () => {
    const client = makeClient();
    const spy = mockSimulate(client, i128(123_456_789_000n));

    await expect(client.balance(ALICE)).resolves.toBe(123_456_789_000n);
    const [method, args] = spy.mock.calls[0] as [string, xdr.ScVal[]];
    expect(method).toBe('balance');
    expect(Address.fromScVal(args[0]).toString()).toBe(ALICE);
  });

  it('balanceAt passes the timestamp as u64', async () => {
    const client = makeClient();
    const spy = mockSimulate(client, i128(42n));

    await expect(client.balanceAt(ALICE, 1_700_000_000)).resolves.toBe(42n);
    const [method, args] = spy.mock.calls[0] as [string, xdr.ScVal[]];
    expect(method).toBe('balance_at');
    expect(args[1].switch().name).toBe('scvU64');
    expect(scValToNative(args[1])).toBe(1_700_000_000n);
  });

  it('totalSupply and allowance decode i128 values', async () => {
    const client = makeClient();
    mockSimulate(client, i128(10n ** 20n));
    await expect(client.totalSupply()).resolves.toBe(10n ** 20n);

    const other = makeClient();
    const spy = mockSimulate(other, i128(500n));
    await expect(other.allowance(ALICE, BOB)).resolves.toBe(500n);
    const [method, args] = spy.mock.calls[0] as [string, xdr.ScVal[]];
    expect(method).toBe('allowance');
    expect(Address.fromScVal(args[0]).toString()).toBe(ALICE);
    expect(Address.fromScVal(args[1]).toString()).toBe(BOB);
  });

  it('metadata reads name, symbol and decimals', async () => {
    const client = makeClient();
    jest.spyOn(client as any, 'simulate').mockImplementation(async (...callArgs: unknown[]) => {
      const method = callArgs[0] as string;
      const retvals: Record<string, xdr.ScVal> = {
        name: xdr.ScVal.scvString('Astera Pool Share'),
        symbol: xdr.ScVal.scvString('aPOOL'),
        decimals: xdr.ScVal.scvU32(7),
      };
      return { result: { retval: retvals[method] } };
    });

    await expect(client.metadata()).resolves.toEqual({
      name: 'Astera Pool Share',
      symbol: 'aPOOL',
      decimals: 7,
    });
  });

  it('isPaused and admin decode bool / address', async () => {
    const client = makeClient();
    mockSimulate(client, xdr.ScVal.scvBool(true));
    await expect(client.isPaused()).resolves.toBe(true);

    mockSimulate(client, new Address(ADMIN).toScVal());
    await expect(client.admin()).resolves.toBe(ADMIN);
  });

  it('throws when simulation returns no result', async () => {
    const client = makeClient();
    jest.spyOn(client as any, 'simulate').mockResolvedValue({});
    await expect(client.totalSupply()).rejects.toThrow('Simulation returned no result');
  });
});

describe('ShareClient writes', () => {
  const signer = async (txXdr: string) => txXdr;

  it('transfer is sourced from `from` and encodes (from, to, i128 amount)', async () => {
    const client = makeClient();
    const spy = mockSend(client);

    await expect(client.transfer({ signer, from: ALICE, to: BOB, amount: 75n })).resolves.toBe(
      'txhash',
    );
    const [caller, method, args] = spy.mock.calls[0] as [string, string, xdr.ScVal[]];
    expect(caller).toBe(ALICE);
    expect(method).toBe('transfer');
    expect(Address.fromScVal(args[0]).toString()).toBe(ALICE);
    expect(Address.fromScVal(args[1]).toString()).toBe(BOB);
    expect(args[2].switch().name).toBe('scvI128');
    expect(scValToNative(args[2])).toBe(75n);
  });

  it('approve encodes the expiration ledger as u32', async () => {
    const client = makeClient();
    const spy = mockSend(client);

    await client.approve({ signer, owner: ALICE, spender: BOB, amount: 10n, expirationLedger: 99 });
    const [caller, method, args] = spy.mock.calls[0] as [string, string, xdr.ScVal[]];
    expect(caller).toBe(ALICE);
    expect(method).toBe('approve');
    expect(args).toHaveLength(4);
    expect(args[3].switch().name).toBe('scvU32');
    expect(scValToNative(args[3])).toBe(99);
  });

  it('transferFrom and burnFrom are sourced from the spender', async () => {
    const client = makeClient();
    const spy = mockSend(client);

    await client.transferFrom({ signer, spender: BOB, from: ALICE, to: ADMIN, amount: 5n });
    await client.burnFrom({ signer, spender: BOB, from: ALICE, amount: 5n });

    expect(spy.mock.calls[0][0]).toBe(BOB);
    expect(spy.mock.calls[0][1]).toBe('transfer_from');
    expect((spy.mock.calls[0][2] as xdr.ScVal[]).length).toBe(4);
    expect(spy.mock.calls[1][0]).toBe(BOB);
    expect(spy.mock.calls[1][1]).toBe('burn_from');
  });

  it('mint is sourced from the admin and only passes (to, amount)', async () => {
    const client = makeClient();
    const spy = mockSend(client);

    await client.mint({ signer, admin: ADMIN, to: ALICE, amount: 1_000n });
    const [caller, method, args] = spy.mock.calls[0] as [string, string, xdr.ScVal[]];
    expect(caller).toBe(ADMIN);
    expect(method).toBe('mint');
    expect(args).toHaveLength(2);
    expect(Address.fromScVal(args[0]).toString()).toBe(ALICE);
    expect(scValToNative(args[1])).toBe(1_000n);
  });

  it('setAdmin is sourced from the current admin and passes only the new admin', async () => {
    const client = makeClient();
    const spy = mockSend(client);

    await client.setAdmin({ signer, admin: ADMIN, newAdmin: BOB });
    const [caller, method, args] = spy.mock.calls[0] as [string, string, xdr.ScVal[]];
    expect(caller).toBe(ADMIN);
    expect(method).toBe('set_admin');
    expect(args).toHaveLength(1);
    expect(Address.fromScVal(args[0]).toString()).toBe(BOB);
  });

  it('initialize encodes (admin, u32 decimals, name, symbol)', async () => {
    const client = makeClient();
    const spy = mockSend(client);

    await client.initialize({ signer, admin: ADMIN, decimals: 7, name: 'Share', symbol: 'SHR' });
    const [, method, args] = spy.mock.calls[0] as [string, string, xdr.ScVal[]];
    expect(method).toBe('initialize');
    expect(args[1].switch().name).toBe('scvU32');
    expect(scValToNative(args[2])).toBe('Share');
    expect(scValToNative(args[3])).toBe('SHR');
  });
});

describe('ShareErrors', () => {
  it('mirrors the share contract error codes', () => {
    expect(ShareErrors[1].message).toBe('ContractPaused');
    expect(ShareErrors[7].message).toBe('InsufficientBalance');
    expect(ShareErrors[12].message).toBe('TotalSupplyUnderflow');
  });
});
