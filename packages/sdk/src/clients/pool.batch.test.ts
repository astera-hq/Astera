import { xdr } from '@stellar/stellar-sdk';
import { PoolClient } from './pool';

describe('PoolClient.getFundedInvoicesBatch (#987)', () => {
  function scvMap(fields: Record<string, xdr.ScVal>): xdr.ScVal {
    return xdr.ScVal.scvMap(
      Object.entries(fields).map(
        ([key, val]) =>
          new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol(key), val }),
      ),
    );
  }

  function fundedInvoiceScVal(invoiceId: number): xdr.ScVal {
    return scvMap({
      invoice_id: xdr.ScVal.scvU64(new xdr.Uint64(invoiceId)),
      sme: xdr.ScVal.scvString('GSME'),
      token: xdr.ScVal.scvString('GTOKEN'),
      principal: xdr.ScVal.scvI128(
        new xdr.Int128Parts({ hi: xdr.Int64.fromString('0'), lo: xdr.Uint64.fromString('1000') }),
      ),
      funded_at: xdr.ScVal.scvU64(new xdr.Uint64(1700000000)),
      due_date: xdr.ScVal.scvU64(new xdr.Uint64(1700086400)),
    });
  }

  it('decodes a batch response preserving null entries for missing invoices', async () => {
    const client = new PoolClient({
      rpcUrl: 'https://example.com',
      network: 'Test SDF Network ; September 2015',
      contractId: 'CPOOL',
    });

    const retval = xdr.ScVal.scvVec([
      fundedInvoiceScVal(1),
      xdr.ScVal.scvVoid(),
      fundedInvoiceScVal(3),
    ]);

    jest.spyOn(client as any, 'simulate').mockResolvedValue({
      result: { retval },
    });

    const result = await client.getFundedInvoicesBatch([1, 2, 3]);

    expect(result).toHaveLength(3);
    expect(result[0]?.invoiceId).toBe(1n);
    expect(result[1]).toBeNull();
    expect(result[2]?.invoiceId).toBe(3n);
  });

  it('propagates a simulation error (e.g. batch-too-large rejection) instead of decoding', async () => {
    const client = new PoolClient({
      rpcUrl: 'https://example.com',
      network: 'Test SDF Network ; September 2015',
      contractId: 'CPOOL',
    });

    jest.spyOn(client as any, 'simulate').mockResolvedValue({
      error: 'HostError: Error(Contract, #BatchTooLarge)',
    });

    await expect(client.getFundedInvoicesBatch([1, 2, 3])).rejects.toThrow('Simulation failed');
  });
});
