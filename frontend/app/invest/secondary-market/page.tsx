'use client';

import { useState } from 'react';
import toast from 'react-hot-toast';
import { mutate } from 'swr';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import { parseStellarAddress } from '@/lib/types';
import type { Listing, ListingKind, Order, OrderSide } from '@/lib/types';
import { NETWORK, toStroops, formatUSDC } from '@/lib/stellar';
import {
  buildListPositionTx,
  buildCancelListingTx,
  buildBuyListingTx,
  buildPlaceOrderTx,
  buildCancelOrderTx,
  buildExpireOrderTx,
} from '@/lib/contracts';
import { useOpenListings, useMyListings, useOrderBook, useMyOrders } from '@/lib/cache';
import { UseTrackTransaction } from '@/hooks/useTrackTransaction';
import { useTranslations } from 'next-intl';

const ORDER_STATUS_STYLES: Record<Order['status'], string> = {
  Open: 'bg-blue-500/20 text-blue-400 border-blue-500/30',
  PartiallyFilled: 'bg-yellow-500/20 text-yellow-400 border-yellow-500/30',
  Filled: 'bg-green-500/20 text-green-400 border-green-500/30',
  Cancelled: 'bg-brand-dark text-brand-muted border-brand-border',
  Expired: 'bg-brand-dark text-brand-muted border-brand-border',
};

function formatOrderPrice(price: bigint): string {
  return formatUSDC(price);
}

const STATUS_STYLES: Record<Listing['status'], string> = {
  Open: 'bg-blue-500/20 text-blue-400 border-blue-500/30',
  Filled: 'bg-green-500/20 text-green-400 border-green-500/30',
  Cancelled: 'bg-brand-dark text-brand-muted border-brand-border',
};

export default function SecondaryMarketPage() {
  const { wallet } = useStore();
  const t = useTranslations('SecondaryMarket');
  const [txLoading, setTxLoading] = useState(false);
  const trackedSubmit = UseTrackTransaction('Secondary Market');

  const { data: openListings = [], isLoading: openLoading } = useOpenListings();
  const { data: myListings = [], isLoading: myListingsLoading } = useMyListings(wallet.address ?? null);

  const [invoiceId, setInvoiceId] = useState('');
  const [kind, setKind] = useState<ListingKind>('SingleFunded');
  const [amountOrBps, setAmountOrBps] = useState('');
  const [price, setPrice] = useState('');

  const [bookInvoiceId, setBookInvoiceId] = useState('');
  const [bookKind, setBookKind] = useState<ListingKind>('SingleFunded');
  const [orderSide, setOrderSide] = useState<OrderSide>('Ask');
  const [orderAmountOrBps, setOrderAmountOrBps] = useState('');
  const [orderPrice, setOrderPrice] = useState('');
  const [orderExpiresHours, setOrderExpiresHours] = useState('');

  const bookInvoiceIdNum = Number(bookInvoiceId);
  const hasValidBookInvoiceId = Number.isFinite(bookInvoiceIdNum) && bookInvoiceIdNum > 0;
  const { data: book, isLoading: bookLoading } = useOrderBook(
    hasValidBookInvoiceId ? bookInvoiceIdNum : null,
    bookKind,
  );
  const { data: myOrders = [], isLoading: myOrdersLoading } = useMyOrders(wallet.address ?? null);

  async function signAndSubmit(xdr: string, label?: string) {
    const freighter = await import('@stellar/freighter-api');
    const { signedTxXdr, error: signError } = await freighter.signTransaction(xdr, {
      networkPassphrase: NETWORK,
      address: wallet.address!,
    });
    if (signError) throw new Error(signError.message || 'Signing rejected.');
    const submitter = label ? UseTrackTransaction(label) : trackedSubmit;
    await submitter(signedTxXdr);
  }

  function refreshListings() {
    mutate('secondary-market-open-listings');
    if (wallet.address) mutate(['secondary-market-my-listings', wallet.address]);
  }

  async function handleCreateListing(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address) return;
    const invoiceIdNum = Number(invoiceId);
    const priceNum = Number(price);
    if (!Number.isFinite(invoiceIdNum) || invoiceIdNum <= 0) {
      toast.error(t('toastInvalidId'));
      return;
    }
    if (!priceNum || priceNum <= 0) {
      toast.error(t('toastInvalidPrice'));
      return;
    }
    let amountOrBpsValue: bigint;
    if (kind === 'CoFunding') {
      const bps = Number(amountOrBps);
      if (!Number.isFinite(bps) || bps <= 0 || bps > 10_000) {
        toast.error(t('toastInvalidBps'));
        return;
      }
      amountOrBpsValue = BigInt(Math.trunc(bps));
    } else {
      const amountNum = Number(amountOrBps);
      if (!Number.isFinite(amountNum) || amountNum <= 0) {
        toast.error(t('toastInvalidAmount'));
        return;
      }
      amountOrBpsValue = toStroops(amountNum);
    }
    setTxLoading(true);
    try {
      const seller = parseStellarAddress(wallet.address);
      const xdr = await buildListPositionTx({ seller, invoiceId: invoiceIdNum, kind, amountOrBps: amountOrBpsValue, price: toStroops(priceNum) });
      await signAndSubmit(xdr, `List invoice #${invoiceIdNum}`);
      toast.success(t('toastListSuccess', { id: invoiceIdNum }));
      setInvoiceId('');
      setAmountOrBps('');
      setPrice('');
      refreshListings();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('toastTxFailed'));
    } finally {
      setTxLoading(false);
    }
  }

  async function handleCancel(listing: Listing) {
    if (!wallet.address) return;
    setTxLoading(true);
    try {
      const seller = parseStellarAddress(wallet.address);
      const xdr = await buildCancelListingTx(seller, listing.listingId);
      await signAndSubmit(xdr, `Cancel listing #${listing.listingId}`);
      toast.success(t('toastCancelSuccess', { id: listing.listingId }));
      refreshListings();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('toastTxFailed'));
    } finally {
      setTxLoading(false);
    }
  }

  async function handleBuy(listing: Listing) {
    if (!wallet.address) return;
    setTxLoading(true);
    try {
      const buyer = parseStellarAddress(wallet.address);
      const xdr = await buildBuyListingTx(buyer, listing.listingId);
      await signAndSubmit(xdr, `Buy listing #${listing.listingId}`);
      toast.success(t('toastBuySuccess', { id: listing.listingId }));
      refreshListings();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('toastTxFailed'));
    } finally {
      setTxLoading(false);
    }
  }

  function refreshOrderBook() {
    if (hasValidBookInvoiceId) {
      mutate(['secondary-market-order-book', bookInvoiceIdNum, bookKind]);
    }
    if (wallet.address) mutate(['secondary-market-my-orders', wallet.address]);
  }

  async function handlePlaceOrder(e: React.FormEvent) {
    e.preventDefault();
    if (!wallet.address) return;
    if (!hasValidBookInvoiceId) {
      toast.error(t('toastInvalidId'));
      return;
    }
    const priceNum = Number(orderPrice);
    if (!priceNum || priceNum <= 0) {
      toast.error(t('toastInvalidPrice'));
      return;
    }
    let amountOrBpsValue: bigint;
    if (bookKind === 'CoFunding') {
      const bps = Number(orderAmountOrBps);
      if (!Number.isFinite(bps) || bps <= 0 || bps > 10_000) {
        toast.error(t('toastInvalidBps'));
        return;
      }
      amountOrBpsValue = BigInt(Math.trunc(bps));
    } else {
      const amountNum = Number(orderAmountOrBps);
      if (!Number.isFinite(amountNum) || amountNum <= 0) {
        toast.error(t('toastInvalidAmount'));
        return;
      }
      amountOrBpsValue = toStroops(amountNum);
    }
    const hours = Number(orderExpiresHours);
    const expiresAt =
      orderExpiresHours && Number.isFinite(hours) && hours > 0
        ? Math.floor(new Date().getTime() / 1000) + Math.round(hours * 3600)
        : 0;
    setTxLoading(true);
    try {
      const owner = parseStellarAddress(wallet.address);
      const xdr = await buildPlaceOrderTx({ owner, invoiceId: bookInvoiceIdNum, kind: bookKind, side: orderSide, amountOrBps: amountOrBpsValue, price: toStroops(priceNum), expiresAt });
      await signAndSubmit(xdr, `Place ${orderSide.toLowerCase()} on invoice #${bookInvoiceIdNum}`);
      toast.success(t('toastOrderSuccess', { id: bookInvoiceIdNum }));
      setOrderAmountOrBps('');
      setOrderPrice('');
      setOrderExpiresHours('');
      refreshOrderBook();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('toastTxFailed'));
    } finally {
      setTxLoading(false);
    }
  }

  async function handleCancelOrder(order: Order) {
    if (!wallet.address) return;
    setTxLoading(true);
    try {
      const owner = parseStellarAddress(wallet.address);
      const xdr = await buildCancelOrderTx(owner, order.orderId);
      await signAndSubmit(xdr, `Cancel order #${order.orderId}`);
      toast.success(t('toastCancelOrderSuccess', { id: order.orderId }));
      refreshOrderBook();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('toastTxFailed'));
    } finally {
      setTxLoading(false);
    }
  }

  async function handleExpireOrder(order: Order) {
    if (!wallet.address) return;
    setTxLoading(true);
    try {
      const caller = parseStellarAddress(wallet.address);
      const xdr = await buildExpireOrderTx(caller, order.orderId);
      await signAndSubmit(xdr, `Expire order #${order.orderId}`);
      toast.success(t('toastExpireOrderSuccess', { id: order.orderId }));
      refreshOrderBook();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('toastTxFailed'));
    } finally {
      setTxLoading(false);
    }
  }

  return (
    <div className="max-w-5xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">{t('title')}</h1>
        <p className="text-brand-muted text-sm">{t('description')}</p>
      </div>

      {/* Create a listing */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
        <h2 className="text-xl font-bold">{t('listPosition')}</h2>
        <form onSubmit={handleCreateListing} className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          <div>
            <label className="block text-sm text-brand-muted mb-1">{t('invoiceId')}</label>
            <input type="number" min="1" value={invoiceId} onChange={(e) => setInvoiceId(e.target.value)} disabled={!wallet.connected || txLoading} placeholder="e.g. 42" className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50" />
          </div>
          <div>
            <label className="block text-sm text-brand-muted mb-1">{t('positionType')}</label>
            <select value={kind} onChange={(e) => setKind(e.target.value as ListingKind)} disabled={!wallet.connected || txLoading} className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50">
              <option value="SingleFunded">{t('poolPosition')}</option>
              <option value="CoFunding">{t('coFundingShare')}</option>
            </select>
          </div>
          <div>
            <label className="block text-sm text-brand-muted mb-1">
              {kind === 'CoFunding' ? t('shareBps') : t('amountUsdc')}
            </label>
            <input type="number" min="0" step={kind === 'CoFunding' ? '1' : '0.01'} max={kind === 'CoFunding' ? 10000 : undefined} value={amountOrBps} onChange={(e) => setAmountOrBps(e.target.value)} disabled={!wallet.connected || txLoading} placeholder={kind === 'CoFunding' ? 'e.g. 2500' : 'e.g. 500.00'} className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50" />
          </div>
          <div>
            <label className="block text-sm text-brand-muted mb-1">{t('priceUsdc')}</label>
            <input type="number" min="0" step="0.01" value={price} onChange={(e) => setPrice(e.target.value)} disabled={!wallet.connected || txLoading} placeholder="e.g. 510.00" className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50" />
          </div>
          <div className="sm:col-span-2">
            <button type="submit" disabled={!wallet.connected || txLoading} className="w-full py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50">
              {txLoading ? t('processing') : t('listPositionBtn')}
            </button>
          </div>
        </form>
      </div>

      {/* Open listings */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">{t('openListings')}</h2>
        {openLoading ? (
          <Skeleton className="h-40 w-full rounded-2xl" />
        ) : openListings.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">{t('noOpenListings')}</div>
        ) : (
          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
            <div className="overflow-x-auto">
              <table className="w-full min-w-[720px] text-left text-sm">
                <thead className="bg-brand-dark border-b border-brand-border text-brand-muted">
                  <tr>
                    <th className="px-6 py-4 font-medium">{t('tableInvoice')}</th>
                    <th className="px-6 py-4 font-medium">{t('tableType')}</th>
                    <th className="px-6 py-4 font-medium">{t('tableAmount')}</th>
                    <th className="px-6 py-4 font-medium">{t('tablePrice')}</th>
                    <th className="px-6 py-4 font-medium text-right">{t('tableActions')}</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-brand-border">
                  {openListings.filter((l) => l.status === 'Open').map((listing) => {
                    const isOwnListing = wallet.address && listing.seller.toLowerCase() === wallet.address.toLowerCase();
                    return (
                      <tr key={listing.listingId} className="hover:bg-brand-dark/50 transition-colors">
                        <td className="px-6 py-4">#{listing.invoiceId}</td>
                        <td className="px-6 py-4">{listing.kind === 'CoFunding' ? t('coFundingShare') : t('poolPosition')}</td>
                        <td className="px-6 py-4">{listing.kind === 'CoFunding' ? `${(Number(listing.amountOrBps) / 100).toFixed(2)}%` : formatUSDC(listing.amountOrBps)}</td>
                        <td className="px-6 py-4">{formatUSDC(listing.price)}</td>
                        <td className="px-6 py-4 text-right">
                          {isOwnListing ? (
                            <span className="text-brand-muted text-xs">{t('yourListing')}</span>
                          ) : (
                            <button onClick={() => handleBuy(listing)} disabled={!wallet.connected || txLoading} className="px-3 py-1.5 bg-brand-gold text-brand-dark rounded-lg text-xs font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50">{t('buy')}</button>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>

      {/* My listings */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">{t('myListings')}</h2>
        {!wallet.connected ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">{t('connectWalletListings')}</div>
        ) : myListingsLoading ? (
          <Skeleton className="h-24 w-full rounded-2xl" />
        ) : myListings.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">{t('noMyListings')}</div>
        ) : (
          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
            <div className="overflow-x-auto">
              <table className="w-full min-w-[680px] text-left text-sm">
                <thead className="bg-brand-dark border-b border-brand-border text-brand-muted">
                  <tr>
                    <th className="px-6 py-4 font-medium">{t('tableInvoice')}</th>
                    <th className="px-6 py-4 font-medium">{t('tableType')}</th>
                    <th className="px-6 py-4 font-medium">{t('tablePrice')}</th>
                    <th className="px-6 py-4 font-medium">{t('tableStatus')}</th>
                    <th className="px-6 py-4 font-medium text-right">{t('tableActions')}</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-brand-border">
                  {myListings.map((listing) => (
                    <tr key={listing.listingId} className="hover:bg-brand-dark/50 transition-colors">
                      <td className="px-6 py-4">#{listing.invoiceId}</td>
                      <td className="px-6 py-4">{listing.kind === 'CoFunding' ? t('coFundingShare') : t('poolPosition')}</td>
                      <td className="px-6 py-4">{formatUSDC(listing.price)}</td>
                      <td className="px-6 py-4">
                        <span className={`px-2 py-0.5 rounded-full text-xs font-semibold border ${STATUS_STYLES[listing.status]}`}>{listing.status}</span>
                      </td>
                      <td className="px-6 py-4 text-right">
                        {listing.status === 'Open' && (
                          <button onClick={() => handleCancel(listing)} disabled={txLoading} className="px-3 py-1.5 bg-red-500/20 text-red-400 border border-red-500/30 rounded-lg text-xs font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50">{t('cancelBtn')}</button>
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>

      {/* Order book */}
      <div>
        <h2 className="text-xl font-bold mb-2">{t('orderBook')}</h2>
        <p className="text-brand-muted text-sm mb-4">{t('orderBookDesc')}</p>
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
          <form onSubmit={handlePlaceOrder} className="grid grid-cols-1 sm:grid-cols-3 gap-4">
            <div>
              <label className="block text-sm text-brand-muted mb-1">{t('invoiceId')}</label>
              <input type="number" min="1" value={bookInvoiceId} onChange={(e) => setBookInvoiceId(e.target.value)} disabled={!wallet.connected || txLoading} placeholder="e.g. 42" className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50" />
            </div>
            <div>
              <label className="block text-sm text-brand-muted mb-1">{t('positionType')}</label>
              <select value={bookKind} onChange={(e) => setBookKind(e.target.value as ListingKind)} disabled={!wallet.connected || txLoading} className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50">
                <option value="SingleFunded">{t('poolPosition')}</option>
                <option value="CoFunding">{t('coFundingShare')}</option>
              </select>
            </div>
            <div>
              <label className="block text-sm text-brand-muted mb-1">{t('side')}</label>
              <select value={orderSide} onChange={(e) => setOrderSide(e.target.value as OrderSide)} disabled={!wallet.connected || txLoading} className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50">
                <option value="Bid">{t('bid')}</option>
                <option value="Ask">{t('ask')}</option>
              </select>
            </div>
            <div>
              <label className="block text-sm text-brand-muted mb-1">
                {bookKind === 'CoFunding' ? t('shareBps') : t('amountUsdc')}
              </label>
              <input type="number" min="0" step={bookKind === 'CoFunding' ? '1' : '0.01'} max={bookKind === 'CoFunding' ? 10000 : undefined} value={orderAmountOrBps} onChange={(e) => setOrderAmountOrBps(e.target.value)} disabled={!wallet.connected || txLoading} placeholder={bookKind === 'CoFunding' ? 'e.g. 2500' : 'e.g. 500.00'} className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50" />
            </div>
            <div>
              <label className="block text-sm text-brand-muted mb-1">{t('pricePerUnit')}</label>
              <input type="number" min="0" step="0.0000001" value={orderPrice} onChange={(e) => setOrderPrice(e.target.value)} disabled={!wallet.connected || txLoading} placeholder="e.g. 1.05" className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50" />
            </div>
            <div>
              <label className="block text-sm text-brand-muted mb-1">{t('expiresHours')}</label>
              <input type="number" min="0" step="1" value={orderExpiresHours} onChange={(e) => setOrderExpiresHours(e.target.value)} disabled={!wallet.connected || txLoading} placeholder={t('expiresNever')} className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50" />
            </div>
            <div className="sm:col-span-3">
              <button type="submit" disabled={!wallet.connected || txLoading} className="w-full py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50">
                {txLoading ? t('processing') : t('placeOrder', { side: orderSide })}
              </button>
            </div>
          </form>

          {!hasValidBookInvoiceId ? (
            <p className="text-brand-muted text-sm">{t('enterInvoiceId')}</p>
          ) : bookLoading ? (
            <Skeleton className="h-32 w-full rounded-xl" />
          ) : (
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
              <div>
                <h3 className="text-sm font-semibold text-brand-muted mb-2">{t('bids', { count: book?.bids.length ?? 0 })}</h3>
                <div className="bg-brand-dark border border-brand-border rounded-xl overflow-hidden">
                  {!book || book.bids.length === 0 ? (
                    <p className="px-4 py-3 text-brand-muted text-xs">{t('noRestingBids')}</p>
                  ) : (
                    <table className="w-full text-left text-xs">
                      <tbody className="divide-y divide-brand-border">
                        {book.bids.map((o) => (
                          <tr key={o.orderId}>
                            <td className="px-4 py-2">{formatOrderPrice(o.price)}</td>
                            <td className="px-4 py-2">{bookKind === 'CoFunding' ? `${(Number(o.quantity) / 100).toFixed(2)}%` : formatUSDC(o.quantity)}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}
                </div>
              </div>
              <div>
                <h3 className="text-sm font-semibold text-brand-muted mb-2">{t('asks', { count: book?.asks.length ?? 0 })}</h3>
                <div className="bg-brand-dark border border-brand-border rounded-xl overflow-hidden">
                  {!book || book.asks.length === 0 ? (
                    <p className="px-4 py-3 text-brand-muted text-xs">{t('noRestingAsks')}</p>
                  ) : (
                    <table className="w-full text-left text-xs">
                      <tbody className="divide-y divide-brand-border">
                        {book.asks.map((o) => (
                          <tr key={o.orderId}>
                            <td className="px-4 py-2">{formatOrderPrice(o.price)}</td>
                            <td className="px-4 py-2">{bookKind === 'CoFunding' ? `${(Number(o.quantity) / 100).toFixed(2)}%` : formatUSDC(o.quantity)}</td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}
                </div>
              </div>
            </div>
          )}
        </div>
      </div>

      {/* My orders */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">{t('myOrders')}</h2>
        {!wallet.connected ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">{t('connectWalletOrders')}</div>
        ) : myOrdersLoading ? (
          <Skeleton className="h-24 w-full rounded-2xl" />
        ) : myOrders.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">{t('noMyOrders')}</div>
        ) : (
          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
            <div className="overflow-x-auto">
              <table className="w-full min-w-[760px] text-left text-sm">
                <thead className="bg-brand-dark border-b border-brand-border text-brand-muted">
                  <tr>
                    <th className="px-6 py-4 font-medium">{t('tableInvoice')}</th>
                    <th className="px-6 py-4 font-medium">{t('tableSide')}</th>
                    <th className="px-6 py-4 font-medium">{t('tablePrice')}</th>
                    <th className="px-6 py-4 font-medium">{t('tableRemaining')}</th>
                    <th className="px-6 py-4 font-medium">{t('tableStatus')}</th>
                    <th className="px-6 py-4 font-medium text-right">{t('tableActions')}</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-brand-border">
                  {myOrders.map((order) => {
                    const canCancel = order.status === 'Open' || order.status === 'PartiallyFilled';
                    const canExpire = canCancel && order.expiresAt !== 0 && order.expiresAt <= new Date().getTime() / 1000;
                    return (
                      <tr key={order.orderId} className="hover:bg-brand-dark/50 transition-colors">
                        <td className="px-6 py-4">#{order.invoiceId}</td>
                        <td className="px-6 py-4">{order.side}</td>
                        <td className="px-6 py-4">{formatOrderPrice(order.price)}</td>
                        <td className="px-6 py-4">{order.kind === 'CoFunding' ? `${(Number(order.remaining) / 100).toFixed(2)}%` : formatUSDC(order.remaining)}</td>
                        <td className="px-6 py-4">
                          <span className={`px-2 py-0.5 rounded-full text-xs font-semibold border ${ORDER_STATUS_STYLES[order.status]}`}>{order.status}</span>
                        </td>
                        <td className="px-6 py-4 text-right space-x-2">
                          {canExpire && (
                            <button onClick={() => handleExpireOrder(order)} disabled={txLoading} className="px-3 py-1.5 bg-brand-dark text-brand-muted border border-brand-border rounded-lg text-xs font-semibold hover:bg-brand-border/50 transition-colors disabled:opacity-50">{t('expireBtn')}</button>
                          )}
                          {canCancel && (
                            <button onClick={() => handleCancelOrder(order)} disabled={txLoading} className="px-3 py-1.5 bg-red-500/20 text-red-400 border border-red-500/30 rounded-lg text-xs font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50">{t('cancelBtn')}</button>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>

      <div className="p-4 bg-brand-dark border border-brand-border rounded-xl text-xs text-brand-muted space-y-1">
        <p>• Only funded positions with no active co-funding round in progress can be listed.</p>
        <p>• Buying a listing debits your available pool balance by the listing price.</p>
        <p>• Cancelling only removes the listing — it does not affect the underlying position.</p>
        <p>• Order-book fills always execute at the resting order&apos;s price, not the incoming order&apos;s — a marketable bid/ask may fill better than its limit price.</p>
      </div>
    </div>
  );
}
