'use client';

import { useState } from 'react';
import toast from 'react-hot-toast';
import { mutate } from 'swr';
import { useStore } from '@/lib/store';
import { Skeleton } from '@/components/Skeleton';
import { parseStellarAddress } from '@/lib/types';
import type { Listing, ListingKind, Order, OrderSide } from '@/lib/types';
import { toStroops, formatUSDC } from '@/lib/stellar';
import {
  buildListPositionTx,
  buildCancelListingTx,
  buildBuyListingTx,
  buildPlaceOrderTx,
  buildCancelOrderTx,
  buildExpireOrderTx,
} from '@/lib/contracts';
import { useOpenListings, useMyListings, useOrderBook, useMyOrders } from '@/lib/cache';
import { useTrackTransaction } from '@/hooks/useTrackTransaction';

const ORDER_STATUS_STYLES: Record<Order['status'], string> = {
  Open: 'bg-blue-500/20 text-blue-400 border-blue-500/30',
  PartiallyFilled: 'bg-yellow-500/20 text-yellow-400 border-yellow-500/30',
  Filled: 'bg-green-500/20 text-green-400 border-green-500/30',
  Cancelled: 'bg-brand-dark text-brand-muted border-brand-border',
  Expired: 'bg-brand-dark text-brand-muted border-brand-border',
};

/** `Order.price` is per-unit scaled by 1e7 — format it as a decimal token price rather than treating it like a flat `Listing.price`. */
function formatOrderPrice(price: bigint): string {
  return formatUSDC(price);
}

const STATUS_STYLES: Record<Listing['status'], string> = {
  Open: 'bg-blue-500/20 text-blue-400 border-blue-500/30',
  Filled: 'bg-green-500/20 text-green-400 border-green-500/30',
  Cancelled: 'bg-brand-dark text-brand-muted border-brand-border',
};

const KIND_LABEL: Record<ListingKind, string> = {
  CoFunding: 'Co-Funding Share',
  SingleFunded: 'Pool Position',
};

export default function SecondaryMarketPage() {
  const { wallet } = useStore();
  const [txLoading, setTxLoading] = useState(false);
  const trackedSubmit = useTrackTransaction('Secondary Market');

  const { data: openListings = [], isLoading: openLoading } = useOpenListings();
  const { data: myListings = [], isLoading: myListingsLoading } = useMyListings(
    wallet.address ?? null,
  );

  const [invoiceId, setInvoiceId] = useState('');
  const [kind, setKind] = useState<ListingKind>('SingleFunded');
  const [amountOrBps, setAmountOrBps] = useState('');
  const [price, setPrice] = useState('');

  // #1035: order book — a separate invoice/kind pair from the listing form
  // above, since a book is looked up per (invoiceId, kind).
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
      networkPassphrase: 'Test SDF Network ; September 2015',
      address: wallet.address!,
    });
    if (signError) throw new Error(signError.message || 'Signing rejected.');
    const submitter = label ? useTrackTransaction(label) : trackedSubmit;
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
      toast.error('Enter a valid invoice ID.');
      return;
    }
    if (!priceNum || priceNum <= 0) {
      toast.error('Enter a valid price.');
      return;
    }

    let amountOrBpsValue: bigint;
    if (kind === 'CoFunding') {
      const bps = Number(amountOrBps);
      if (!Number.isFinite(bps) || bps <= 0 || bps > 10_000) {
        toast.error('Bps must be between 1 and 10000.');
        return;
      }
      amountOrBpsValue = BigInt(Math.trunc(bps));
    } else {
      const amountNum = Number(amountOrBps);
      if (!Number.isFinite(amountNum) || amountNum <= 0) {
        toast.error('Enter a valid amount.');
        return;
      }
      amountOrBpsValue = toStroops(amountNum);
    }

    setTxLoading(true);
    try {
      const seller = parseStellarAddress(wallet.address);
      const xdr = await buildListPositionTx({
        seller,
        invoiceId: invoiceIdNum,
        kind,
        amountOrBps: amountOrBpsValue,
        price: toStroops(priceNum),
      });
      await signAndSubmit(xdr, `List invoice #${invoiceIdNum}`);
      toast.success(`Listed a position on invoice #${invoiceIdNum}.`);
      setInvoiceId('');
      setAmountOrBps('');
      setPrice('');
      refreshListings();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Transaction failed.');
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
      toast.success(`Cancelled listing #${listing.listingId}.`);
      refreshListings();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Transaction failed.');
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
      toast.success(`Bought listing #${listing.listingId}.`);
      refreshListings();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Transaction failed.');
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
      toast.error('Enter a valid invoice ID.');
      return;
    }
    const priceNum = Number(orderPrice);
    if (!priceNum || priceNum <= 0) {
      toast.error('Enter a valid price.');
      return;
    }

    let amountOrBpsValue: bigint;
    if (bookKind === 'CoFunding') {
      const bps = Number(orderAmountOrBps);
      if (!Number.isFinite(bps) || bps <= 0 || bps > 10_000) {
        toast.error('Bps must be between 1 and 10000.');
        return;
      }
      amountOrBpsValue = BigInt(Math.trunc(bps));
    } else {
      const amountNum = Number(orderAmountOrBps);
      if (!Number.isFinite(amountNum) || amountNum <= 0) {
        toast.error('Enter a valid amount.');
        return;
      }
      amountOrBpsValue = toStroops(amountNum);
    }

    const hours = Number(orderExpiresHours);
    const expiresAt =
      orderExpiresHours && Number.isFinite(hours) && hours > 0
        ? Math.floor(Date.now() / 1000) + Math.round(hours * 3600)
        : 0;

    setTxLoading(true);
    try {
      const owner = parseStellarAddress(wallet.address);
      const xdr = await buildPlaceOrderTx({
        owner,
        invoiceId: bookInvoiceIdNum,
        kind: bookKind,
        side: orderSide,
        amountOrBps: amountOrBpsValue,
        // Per-unit price scaled by 1e7 (matches USDC's own stroop scale).
        price: toStroops(priceNum),
        expiresAt,
      });
      await signAndSubmit(xdr, `Place ${orderSide.toLowerCase()} on invoice #${bookInvoiceIdNum}`);
      toast.success(`Order placed on invoice #${bookInvoiceIdNum}.`);
      setOrderAmountOrBps('');
      setOrderPrice('');
      setOrderExpiresHours('');
      refreshOrderBook();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Transaction failed.');
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
      toast.success(`Cancelled order #${order.orderId}.`);
      refreshOrderBook();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Transaction failed.');
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
      toast.success(`Expired order #${order.orderId}.`);
      refreshOrderBook();
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Transaction failed.');
    } finally {
      setTxLoading(false);
    }
  }

  return (
    <div className="max-w-5xl space-y-8">
      <div>
        <h1 className="text-3xl font-bold mb-2">Secondary Market</h1>
        <p className="text-brand-muted text-sm">
          List part or all of a funded position — a pool deposit slice or a co-funding share — for
          sale at a flat price, or buy an open listing from another investor.
        </p>
      </div>

      {/* Create a listing */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
        <h2 className="text-xl font-bold">List a Position</h2>
        <form onSubmit={handleCreateListing} className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          <div>
            <label className="block text-sm text-brand-muted mb-1">Invoice ID</label>
            <input
              type="number"
              min="1"
              value={invoiceId}
              onChange={(e) => setInvoiceId(e.target.value)}
              disabled={!wallet.connected || txLoading}
              placeholder="e.g. 42"
              className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50"
            />
          </div>
          <div>
            <label className="block text-sm text-brand-muted mb-1">Position Type</label>
            <select
              value={kind}
              onChange={(e) => setKind(e.target.value as ListingKind)}
              disabled={!wallet.connected || txLoading}
              className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50"
            >
              <option value="SingleFunded">Pool Position (deployed principal)</option>
              <option value="CoFunding">Co-Funding Share (bps)</option>
            </select>
          </div>
          <div>
            <label className="block text-sm text-brand-muted mb-1">
              {kind === 'CoFunding' ? 'Share (basis points, 1-10000)' : 'Amount (USDC)'}
            </label>
            <input
              type="number"
              min="0"
              step={kind === 'CoFunding' ? '1' : '0.01'}
              max={kind === 'CoFunding' ? 10000 : undefined}
              value={amountOrBps}
              onChange={(e) => setAmountOrBps(e.target.value)}
              disabled={!wallet.connected || txLoading}
              placeholder={kind === 'CoFunding' ? 'e.g. 2500' : 'e.g. 500.00'}
              className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50"
            />
          </div>
          <div>
            <label className="block text-sm text-brand-muted mb-1">Price (USDC)</label>
            <input
              type="number"
              min="0"
              step="0.01"
              value={price}
              onChange={(e) => setPrice(e.target.value)}
              disabled={!wallet.connected || txLoading}
              placeholder="e.g. 510.00"
              className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50"
            />
          </div>
          <div className="sm:col-span-2">
            <button
              type="submit"
              disabled={!wallet.connected || txLoading}
              className="w-full py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
            >
              {txLoading ? 'Processing…' : 'List Position'}
            </button>
          </div>
        </form>
      </div>

      {/* Open listings */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">Open Listings</h2>
        {openLoading ? (
          <Skeleton className="h-40 w-full rounded-2xl" />
        ) : openListings.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            No open listings right now.
          </div>
        ) : (
          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
            <table className="w-full text-left text-sm">
              <thead className="bg-brand-dark border-b border-brand-border text-brand-muted">
                <tr>
                  <th className="px-6 py-4 font-medium">Invoice</th>
                  <th className="px-6 py-4 font-medium">Type</th>
                  <th className="px-6 py-4 font-medium">Amount</th>
                  <th className="px-6 py-4 font-medium">Price</th>
                  <th className="px-6 py-4 font-medium text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-brand-border">
                {openListings
                  .filter((l) => l.status === 'Open')
                  .map((listing) => {
                    const isOwnListing =
                      wallet.address &&
                      listing.seller.toLowerCase() === wallet.address.toLowerCase();
                    return (
                      <tr
                        key={listing.listingId}
                        className="hover:bg-brand-dark/50 transition-colors"
                      >
                        <td className="px-6 py-4">#{listing.invoiceId}</td>
                        <td className="px-6 py-4">{KIND_LABEL[listing.kind]}</td>
                        <td className="px-6 py-4">
                          {listing.kind === 'CoFunding'
                            ? `${(Number(listing.amountOrBps) / 100).toFixed(2)}%`
                            : formatUSDC(listing.amountOrBps)}
                        </td>
                        <td className="px-6 py-4">{formatUSDC(listing.price)}</td>
                        <td className="px-6 py-4 text-right">
                          {isOwnListing ? (
                            <span className="text-brand-muted text-xs">Your listing</span>
                          ) : (
                            <button
                              onClick={() => handleBuy(listing)}
                              disabled={!wallet.connected || txLoading}
                              className="px-3 py-1.5 bg-brand-gold text-brand-dark rounded-lg text-xs font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
                            >
                              Buy
                            </button>
                          )}
                        </td>
                      </tr>
                    );
                  })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* My listings */}
      <div className="space-y-4">
        <h2 className="text-xl font-bold">My Listings</h2>
        {!wallet.connected ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            Connect your wallet to see your listings.
          </div>
        ) : myListingsLoading ? (
          <Skeleton className="h-24 w-full rounded-2xl" />
        ) : myListings.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            You haven&apos;t listed any positions yet.
          </div>
        ) : (
          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
            <table className="w-full text-left text-sm">
              <thead className="bg-brand-dark border-b border-brand-border text-brand-muted">
                <tr>
                  <th className="px-6 py-4 font-medium">Invoice</th>
                  <th className="px-6 py-4 font-medium">Type</th>
                  <th className="px-6 py-4 font-medium">Price</th>
                  <th className="px-6 py-4 font-medium">Status</th>
                  <th className="px-6 py-4 font-medium text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-brand-border">
                {myListings.map((listing) => (
                  <tr key={listing.listingId} className="hover:bg-brand-dark/50 transition-colors">
                    <td className="px-6 py-4">#{listing.invoiceId}</td>
                    <td className="px-6 py-4">{KIND_LABEL[listing.kind]}</td>
                    <td className="px-6 py-4">{formatUSDC(listing.price)}</td>
                    <td className="px-6 py-4">
                      <span
                        className={`px-2 py-0.5 rounded-full text-xs font-semibold border ${STATUS_STYLES[listing.status]}`}
                      >
                        {listing.status}
                      </span>
                    </td>
                    <td className="px-6 py-4 text-right">
                      {listing.status === 'Open' && (
                        <button
                          onClick={() => handleCancel(listing)}
                          disabled={txLoading}
                          className="px-3 py-1.5 bg-red-500/20 text-red-400 border border-red-500/30 rounded-lg text-xs font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50"
                        >
                          Cancel
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Order book */}
      <div>
        <h2 className="text-xl font-bold mb-2">Order Book</h2>
        <p className="text-brand-muted text-sm mb-4">
          Place a resting bid or ask for an invoice&apos;s position. Orders match immediately
          against crossing resting orders — best price first — and any unfilled remainder rests on
          the book until filled, cancelled, or expired.
        </p>

        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl space-y-4">
          <form onSubmit={handlePlaceOrder} className="grid grid-cols-1 sm:grid-cols-3 gap-4">
            <div>
              <label className="block text-sm text-brand-muted mb-1">Invoice ID</label>
              <input
                type="number"
                min="1"
                value={bookInvoiceId}
                onChange={(e) => setBookInvoiceId(e.target.value)}
                disabled={!wallet.connected || txLoading}
                placeholder="e.g. 42"
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50"
              />
            </div>
            <div>
              <label className="block text-sm text-brand-muted mb-1">Position Type</label>
              <select
                value={bookKind}
                onChange={(e) => setBookKind(e.target.value as ListingKind)}
                disabled={!wallet.connected || txLoading}
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50"
              >
                <option value="SingleFunded">Pool Position (deployed principal)</option>
                <option value="CoFunding">Co-Funding Share (bps)</option>
              </select>
            </div>
            <div>
              <label className="block text-sm text-brand-muted mb-1">Side</label>
              <select
                value={orderSide}
                onChange={(e) => setOrderSide(e.target.value as OrderSide)}
                disabled={!wallet.connected || txLoading}
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50"
              >
                <option value="Bid">Bid (buy)</option>
                <option value="Ask">Ask (sell)</option>
              </select>
            </div>
            <div>
              <label className="block text-sm text-brand-muted mb-1">
                {bookKind === 'CoFunding' ? 'Share (basis points, 1-10000)' : 'Amount (USDC)'}
              </label>
              <input
                type="number"
                min="0"
                step={bookKind === 'CoFunding' ? '1' : '0.01'}
                max={bookKind === 'CoFunding' ? 10000 : undefined}
                value={orderAmountOrBps}
                onChange={(e) => setOrderAmountOrBps(e.target.value)}
                disabled={!wallet.connected || txLoading}
                placeholder={bookKind === 'CoFunding' ? 'e.g. 2500' : 'e.g. 500.00'}
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50"
              />
            </div>
            <div>
              <label className="block text-sm text-brand-muted mb-1">Price per unit (USDC)</label>
              <input
                type="number"
                min="0"
                step="0.0000001"
                value={orderPrice}
                onChange={(e) => setOrderPrice(e.target.value)}
                disabled={!wallet.connected || txLoading}
                placeholder="e.g. 1.05"
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50"
              />
            </div>
            <div>
              <label className="block text-sm text-brand-muted mb-1">
                Expires in (hours, optional)
              </label>
              <input
                type="number"
                min="0"
                step="1"
                value={orderExpiresHours}
                onChange={(e) => setOrderExpiresHours(e.target.value)}
                disabled={!wallet.connected || txLoading}
                placeholder="Never"
                className="w-full bg-brand-dark border border-brand-border rounded-xl px-4 py-2.5 text-white placeholder-brand-muted focus:outline-none focus:border-brand-gold text-sm disabled:opacity-50"
              />
            </div>
            <div className="sm:col-span-3">
              <button
                type="submit"
                disabled={!wallet.connected || txLoading}
                className="w-full py-2.5 bg-brand-gold text-brand-dark rounded-xl text-sm font-semibold hover:bg-brand-amber transition-colors disabled:opacity-50"
              >
                {txLoading ? 'Processing…' : `Place ${orderSide}`}
              </button>
            </div>
          </form>

          {!hasValidBookInvoiceId ? (
            <p className="text-brand-muted text-sm">Enter an invoice ID above to view its book.</p>
          ) : bookLoading ? (
            <Skeleton className="h-32 w-full rounded-xl" />
          ) : (
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
              <div>
                <h3 className="text-sm font-semibold text-brand-muted mb-2">
                  Bids ({book?.bids.length ?? 0})
                </h3>
                <div className="bg-brand-dark border border-brand-border rounded-xl overflow-hidden">
                  {!book || book.bids.length === 0 ? (
                    <p className="px-4 py-3 text-brand-muted text-xs">No resting bids.</p>
                  ) : (
                    <table className="w-full text-left text-xs">
                      <tbody className="divide-y divide-brand-border">
                        {book.bids.map((o) => (
                          <tr key={o.orderId}>
                            <td className="px-4 py-2">{formatOrderPrice(o.price)}</td>
                            <td className="px-4 py-2">
                              {bookKind === 'CoFunding'
                                ? `${(Number(o.remaining) / 100).toFixed(2)}%`
                                : formatUSDC(o.remaining)}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  )}
                </div>
              </div>
              <div>
                <h3 className="text-sm font-semibold text-brand-muted mb-2">
                  Asks ({book?.asks.length ?? 0})
                </h3>
                <div className="bg-brand-dark border border-brand-border rounded-xl overflow-hidden">
                  {!book || book.asks.length === 0 ? (
                    <p className="px-4 py-3 text-brand-muted text-xs">No resting asks.</p>
                  ) : (
                    <table className="w-full text-left text-xs">
                      <tbody className="divide-y divide-brand-border">
                        {book.asks.map((o) => (
                          <tr key={o.orderId}>
                            <td className="px-4 py-2">{formatOrderPrice(o.price)}</td>
                            <td className="px-4 py-2">
                              {bookKind === 'CoFunding'
                                ? `${(Number(o.remaining) / 100).toFixed(2)}%`
                                : formatUSDC(o.remaining)}
                            </td>
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
        <h2 className="text-xl font-bold">My Orders</h2>
        {!wallet.connected ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            Connect your wallet to see your orders.
          </div>
        ) : myOrdersLoading ? (
          <Skeleton className="h-24 w-full rounded-2xl" />
        ) : myOrders.length === 0 ? (
          <div className="p-6 bg-brand-card border border-brand-border rounded-2xl text-center text-brand-muted text-sm">
            You haven&apos;t placed any orders yet.
          </div>
        ) : (
          <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
            <table className="w-full text-left text-sm">
              <thead className="bg-brand-dark border-b border-brand-border text-brand-muted">
                <tr>
                  <th className="px-6 py-4 font-medium">Invoice</th>
                  <th className="px-6 py-4 font-medium">Side</th>
                  <th className="px-6 py-4 font-medium">Price</th>
                  <th className="px-6 py-4 font-medium">Remaining</th>
                  <th className="px-6 py-4 font-medium">Status</th>
                  <th className="px-6 py-4 font-medium text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-brand-border">
                {myOrders.map((order) => {
                  const canCancel = order.status === 'Open' || order.status === 'PartiallyFilled';
                  const canExpire =
                    canCancel && order.expiresAt !== 0 && order.expiresAt <= Date.now() / 1000;
                  return (
                    <tr key={order.orderId} className="hover:bg-brand-dark/50 transition-colors">
                      <td className="px-6 py-4">#{order.invoiceId}</td>
                      <td className="px-6 py-4">{order.side}</td>
                      <td className="px-6 py-4">{formatOrderPrice(order.price)}</td>
                      <td className="px-6 py-4">
                        {order.kind === 'CoFunding'
                          ? `${(Number(order.remaining) / 100).toFixed(2)}%`
                          : formatUSDC(order.remaining)}
                      </td>
                      <td className="px-6 py-4">
                        <span
                          className={`px-2 py-0.5 rounded-full text-xs font-semibold border ${ORDER_STATUS_STYLES[order.status]}`}
                        >
                          {order.status}
                        </span>
                      </td>
                      <td className="px-6 py-4 text-right space-x-2">
                        {canExpire && (
                          <button
                            onClick={() => handleExpireOrder(order)}
                            disabled={txLoading}
                            className="px-3 py-1.5 bg-brand-dark text-brand-muted border border-brand-border rounded-lg text-xs font-semibold hover:bg-brand-border/50 transition-colors disabled:opacity-50"
                          >
                            Expire
                          </button>
                        )}
                        {canCancel && (
                          <button
                            onClick={() => handleCancelOrder(order)}
                            disabled={txLoading}
                            className="px-3 py-1.5 bg-red-500/20 text-red-400 border border-red-500/30 rounded-lg text-xs font-semibold hover:bg-red-500/30 transition-colors disabled:opacity-50"
                          >
                            Cancel
                          </button>
                        )}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      <div className="p-4 bg-brand-dark border border-brand-border rounded-xl text-xs text-brand-muted space-y-1">
        <p>• Only funded positions with no active co-funding round in progress can be listed.</p>
        <p>• Buying a listing debits your available pool balance by the listing price.</p>
        <p>• Cancelling only removes the listing — it does not affect the underlying position.</p>
        <p>
          • Order-book fills always execute at the resting order&apos;s price, not the incoming
          order&apos;s — a marketable bid/ask may fill better than its limit price.
        </p>
      </div>
    </div>
  );
}
