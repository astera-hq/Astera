import { xdr } from '@stellar/stellar-sdk';
import { InvoiceClient } from './invoice';

describe('InvoiceClient.getMultipleInvoices (#987)', () => {
  function invoiceScVal(id: number): xdr.ScVal {
    return xdr.ScVal.scvMap(
      Object.entries({
        id: xdr.ScVal.scvU64(new xdr.Uint64(id)),
        owner: xdr.ScVal.scvString('GOWNER'),
        debtor: xdr.ScVal.scvString('GDEBTOR'),
        amount: xdr.ScVal.scvI128(
          new xdr.Int128Parts({ hi: xdr.Int64.fromString('0'), lo: xdr.Uint64.fromString('500') }),
        ),
        due_date: xdr.ScVal.scvU64(new xdr.Uint64(1700086400)),
        description: xdr.ScVal.scvString('test invoice'),
        status: xdr.ScVal.scvSymbol('Pending'),
        created_at: xdr.ScVal.scvU64(new xdr.Uint64(1700000000)),
        funded_at: xdr.ScVal.scvU64(new xdr.Uint64(0)),
        paid_at: xdr.ScVal.scvU64(new xdr.Uint64(0)),
        pool_contract: xdr.ScVal.scvString('CPOOL'),
      }).map(([key, val]) => new xdr.ScMapEntry({ key: xdr.ScVal.scvSymbol(key), val })),
    );
  }

  it('decodes a batch of invoices in request order', async () => {
    const client = new InvoiceClient({
      rpcUrl: 'https://example.com',
      network: 'Test SDF Network ; September 2015',
      contractId: 'CINVOICE',
    });

    const retval = xdr.ScVal.scvVec([invoiceScVal(1), invoiceScVal(2)]);

    jest.spyOn(client as any, 'simulate').mockResolvedValue({
      result: { retval },
    });

    const result = await client.getMultipleInvoices([1, 2]);

    expect(result).toHaveLength(2);
    expect(result[0].id).toBe(1n);
    expect(result[1].id).toBe(2n);
  });

  it('propagates a simulation error instead of decoding', async () => {
    const client = new InvoiceClient({
      rpcUrl: 'https://example.com',
      network: 'Test SDF Network ; September 2015',
      contractId: 'CINVOICE',
    });

    jest.spyOn(client as any, 'simulate').mockResolvedValue({
      error: 'HostError: some failure',
    });

    await expect(client.getMultipleInvoices([1, 2])).rejects.toThrow('Simulation failed');
  });
});
