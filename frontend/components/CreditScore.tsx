'use client';

import { useMemo, useState } from 'react';

interface PaymentRecord {
  invoiceId: number;
  amount: bigint;
  dueDate: number;
  paidDate: number | null;
  status: 'OnTime' | 'Late' | 'Defaulted';
  daysLate?: number;
}

interface Props {
  paid: number;
  funded: number;
  defaulted: number;
  totalVolume: bigint;
  paymentHistory?: PaymentRecord[];
  previousScore?: number;
}

const SCORE_TIERS = [
  { name: 'Very Poor', min: 200 },
  { name: 'Poor', min: 500 },
  { name: 'Fair', min: 580 },
  { name: 'Good', min: 670 },
  { name: 'Very Good', min: 740 },
  { name: 'Excellent', min: 800 },
];

const PTS_PAID_ON_TIME = 30;
const PTS_PAID_LATE = 15;
const PTS_DEFAULTED = 50;
const PTS_NEW_INVOICE = 5;

const TIER_BENEFITS: Record<string, string> = {
  Poor: 'Unlock basic funding limits ($5,000 per invoice)',
  Fair: 'Increase per-invoice limit to $10,000 and reduce factoring fees',
  Good: 'Unlock higher funding limits ($25,000) and preferential fee tiers',
  'Very Good': 'Access premium funding ($50,000+) and lowest factoring fees',
  Excellent: 'Maximum funding limits and exclusive pool access',
};

export default function CreditScore({
  paid,
  funded,
  defaulted,
  totalVolume,
  paymentHistory = [],
  previousScore,
}: Props) {
  const total = paid + funded + defaulted;
  const [showImprovement, setShowImprovement] = useState(false);

  const repaymentRate = total > 0 ? paid / total : 0;
  const volumeBonus = Math.min(Number(totalVolume) / 1e10, 50);
  const score = Math.round(300 + repaymentRate * 500 + volumeBonus);

  const scoreChange = previousScore ? score - previousScore : null;
  const scoreColor =
    score >= 750 ? 'text-green-400' : score >= 600 ? 'text-yellow-400' : 'text-red-400';
  const scoreLabel =
    score >= 750 ? 'Excellent' : score >= 650 ? 'Good' : score >= 550 ? 'Fair' : 'Building';

  const arc = Math.round(((score - 300) / 550) * 180);

  const avgPaymentDays = useMemo(() => {
    if (paymentHistory.length === 0) return 0;
    const onTimePaid = paymentHistory.filter((p) => p.status === 'OnTime');
    if (onTimePaid.length === 0) return 0;
    const totalDays = onTimePaid.reduce((sum, p) => {
      const due = p.dueDate;
      const paid = p.paidDate || Date.now() / 1000;
      return sum + Math.floor((paid - due) / 86400);
    }, 0);
    return Math.round(totalDays / onTimePaid.length);
  }, [paymentHistory]);

  const nextTier = useMemo(() => {
    for (const tier of SCORE_TIERS) {
      if (score < tier.min) {
        return tier;
      }
    }
    return null;
  }, [score]);

  const currentTier = useMemo(() => {
    let tier = SCORE_TIERS[0];
    for (const t of SCORE_TIERS) {
      if (score >= t.min) {
        tier = t;
      }
    }
    return tier;
  }, [score]);

  const pointsToNext = nextTier ? nextTier.min - score : 0;

  const recommendations = useMemo(() => {
    const recs: { factor: string; stat: string; impact: string; tip: string }[] = [];

    const onTimeRate = total > 0 ? paid / total : 0;
    recs.push({
      factor: 'On-time payments',
      stat: `${paid}/${total} (${Math.round(onTimeRate * 100)}%)`,
      impact: 'High',
      tip:
        onTimeRate < 0.8
          ? `Pay your next ${Math.ceil((0.8 * total - paid))} invoices on time to reach Good tier`
          : 'Keep up the great on-time payment record',
    });

    recs.push({
      factor: 'Defaults',
      stat: defaulted > 0 ? `${defaulted} default${defaulted > 1 ? 's' : ''}` : 'None',
      impact: 'High',
      tip:
        defaulted > 0
          ? 'Avoid defaults — each costs 50 points'
          : 'No defaults — maintain this record',
    });

    const lateCount = paymentHistory.filter((p) => p.status === 'Late').length;
    recs.push({
      factor: 'Payment speed',
      stat:
        avgPaymentDays > 0
          ? avgPaymentDays > 0
            ? `Avg ${avgPaymentDays} days ${avgPaymentDays > 0 ? 'late' : 'early'}`
            : 'N/A'
          : lateCount > 0
            ? `${lateCount} late payment(s)`
            : 'No data',
      impact: 'Medium',
      tip:
        avgPaymentDays > 0
          ? 'Try to pay within the due date to boost your score'
          : 'Pay invoices as early as possible for maximum points',
    });

    recs.push({
      factor: 'Invoice volume',
      stat: `${total} total`,
      impact: 'Low',
      tip:
        total < 5
          ? `Create ${5 - total} more invoices to build history faster (+${PTS_NEW_INVOICE} pts at 5 invoices)`
          : total < 10
            ? 'Reach 10 invoices for another score bonus'
            : 'Strong invoice history — keep building',
    });

    return recs;
  }, [paid, defaulted, total, avgPaymentDays, paymentHistory]);

  return (
    <div className="space-y-6">
      {/* Score Overview Card */}
      <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
        <h2 className="text-lg font-semibold mb-6">On-Chain Credit Score</h2>

        {/* Score display with change indicator */}
        <div className="text-center mb-8">
          <div className={`text-5xl font-bold mb-2 ${scoreColor}`}>{score}</div>
          <div className="text-brand-muted text-sm mb-1">{scoreLabel}</div>
          {scoreChange !== null && (
            <div
              className={`text-xs font-medium ${
                scoreChange >= 0 ? 'text-green-400' : 'text-red-400'
              }`}
            >
              {scoreChange >= 0 ? '+' : ''}{scoreChange} since last invoice
            </div>
          )}
          <div className="text-xs text-brand-muted/60 mt-2">Based on {total} invoice(s)</div>
        </div>

        {/* Score Breakdown */}
        <div className="space-y-3">
          <ScoreRow label="Paid on time" count={paid} color="bg-green-500" total={total} />
          <ScoreRow label="Currently funded" count={funded} color="bg-blue-500" total={total} />
          <ScoreRow label="Defaulted" count={defaulted} color="bg-red-500" total={total} />
        </div>

        {/* Additional Stats */}
        {total > 0 && (
          <div className="mt-6 pt-6 border-t border-brand-border grid grid-cols-2 gap-4">
            <div>
              <div className="text-xs text-brand-muted mb-1">Repayment Rate</div>
              <div className="text-lg font-semibold">
                {Math.round(repaymentRate * 100)}%
              </div>
            </div>
            <div>
              <div className="text-xs text-brand-muted mb-1">Average Payment Days</div>
              <div className="text-lg font-semibold">{avgPaymentDays} days</div>
            </div>
          </div>
        )}

        {total === 0 && (
          <p className="text-center text-brand-muted text-sm mt-4">
            Create and repay invoices to build your score.
          </p>
        )}

        {/* Score Tier Progress Bar */}
        {nextTier && (
          <div className="mt-6 pt-6 border-t border-brand-border">
            <div className="flex justify-between text-xs text-brand-muted mb-2">
              <span>
                {currentTier.name} ({score})
              </span>
              <span>
                {nextTier.name} ({nextTier.min})
              </span>
            </div>
            <div className="relative h-3 bg-brand-border rounded-full overflow-hidden">
              <div
                className="h-full bg-gradient-to-r from-brand-border to-green-500 rounded-full transition-all"
                style={{
                  width: `${
                    ((score - currentTier.min) / (nextTier.min - currentTier.min)) * 100
                  }%`,
                }}
              />
              <div
                className="absolute top-0 h-full w-0.5 bg-white/50"
                style={{
                  left: `${
                    ((score - currentTier.min) / (nextTier.min - currentTier.min)) * 100
                  }%`,
                }}
              />
            </div>
            <div className="text-xs text-brand-muted mt-2 text-center">
              {pointsToNext} pts to {nextTier.name}
            </div>
          </div>
        )}
      </div>

      {/* Score Improvement Panel (collapsed by default) */}
      <div className="bg-brand-card border border-brand-border rounded-2xl overflow-hidden">
        <button
          onClick={() => setShowImprovement(!showImprovement)}
          className="w-full p-6 text-left flex items-center justify-between hover:bg-brand-border/20 transition"
        >
          <h3 className="text-lg font-semibold">Improve your score</h3>
          <svg
            className={`w-5 h-5 text-brand-muted transition-transform ${
              showImprovement ? 'rotate-180' : ''
            }`}
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
          >
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
          </svg>
        </button>

        {showImprovement && (
          <div className="px-6 pb-6 space-y-6">
            {/* Score Breakdown Table */}
            <div>
              <h4 className="text-sm font-semibold text-brand-muted uppercase tracking-wider mb-3">
                Current Stats vs. Recommendations
              </h4>
              <div className="overflow-x-auto">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="border-b border-brand-border">
                      <th className="text-left text-brand-muted py-2 px-2">Factor</th>
                      <th className="text-left text-brand-muted py-2 px-2">Your Stats</th>
                      <th className="text-left text-brand-muted py-2 px-2">Impact</th>
                      <th className="text-left text-brand-muted py-2 px-2">Tip</th>
                    </tr>
                  </thead>
                  <tbody>
                    {recommendations.map((rec, i) => (
                      <tr key={i} className="border-b border-brand-border/50">
                        <td className="py-3 px-2 font-medium">{rec.factor}</td>
                        <td className="py-3 px-2 text-brand-muted">{rec.stat}</td>
                        <td className="py-3 px-2">
                          <span
                            className={`inline-block px-2 py-0.5 rounded text-xs font-medium ${
                              rec.impact === 'High'
                                ? 'bg-red-500/20 text-red-400'
                                : rec.impact === 'Medium'
                                  ? 'bg-yellow-500/20 text-yellow-400'
                                  : 'bg-blue-500/20 text-blue-400'
                            }`}
                          >
                            {rec.impact}
                          </span>
                        </td>
                        <td className="py-3 px-2 text-brand-muted text-xs">{rec.tip}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>

            {/* Next Milestone */}
            {nextTier && (
              <div className="p-4 bg-brand-border/30 rounded-xl">
                <h4 className="text-sm font-semibold mb-2">Next Milestone: {nextTier.name}</h4>
                <div className="flex items-center gap-3 mb-3">
                  <div className="text-2xl font-bold">{nextTier.min}</div>
                  <div className="text-xs text-brand-muted">
                    {pointsToNext} points away
                  </div>
                </div>
                <p className="text-sm text-brand-muted">
                  {TIER_BENEFITS[nextTier.name] || 'Unlock new benefits at this tier'}
                </p>
                {pointsToNext <= PTS_PAID_ON_TIME * 2 && (
                  <p className="text-sm text-green-400 mt-2">
                    Pay {Math.ceil(pointsToNext / PTS_PAID_ON_TIME)} more invoice{Math.ceil(pointsToNext / PTS_PAID_ON_TIME) > 1 ? 's' : ''} on time (+{Math.ceil(pointsToNext / PTS_PAID_ON_TIME) * PTS_PAID_ON_TIME} pts) to reach {nextTier.name} tier and {TIER_BENEFITS[nextTier.name]?.toLowerCase()}
                  </p>
                )}
              </div>
            )}
          </div>
        )}
      </div>

      {/* Payment History Table */}
      {paymentHistory.length > 0 && (
        <div className="p-6 bg-brand-card border border-brand-border rounded-2xl">
          <h3 className="text-lg font-semibold mb-4">Payment History</h3>
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead>
                <tr className="border-b border-brand-border">
                  <th className="text-left text-brand-muted py-3 px-2">Invoice</th>
                  <th className="text-right text-brand-muted py-3 px-2">Amount</th>
                  <th className="text-left text-brand-muted py-3 px-2">Status</th>
                  <th className="text-right text-brand-muted py-3 px-2">Days Late</th>
                </tr>
              </thead>
              <tbody>
                {paymentHistory.slice(0, 10).map((record) => (
                  <tr
                    key={record.invoiceId}
                    className="border-b border-brand-border hover:bg-brand-border/30 transition"
                  >
                    <td className="py-3 px-2 font-medium">#{record.invoiceId}</td>
                    <td className="text-right py-3 px-2">
                      ${(Number(record.amount) / 1e6).toFixed(2)}
                    </td>
                    <td className="py-3 px-2">
                      <span
                        className={`inline-block px-2 py-1 rounded text-xs font-medium ${
                          record.status === 'OnTime'
                            ? 'bg-green-500/20 text-green-400'
                            : record.status === 'Late'
                              ? 'bg-yellow-500/20 text-yellow-400'
                              : 'bg-red-500/20 text-red-400'
                        }`}
                      >
                        {record.status === 'OnTime' ? 'On Time' : record.status}
                      </span>
                    </td>
                    <td className="text-right py-3 px-2">
                      {record.daysLate ? record.daysLate : '—'}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}

function ScoreRow({
  label,
  count,
  color,
  total,
}: {
  label: string;
  count: number;
  color: string;
  total: number;
}) {
  const pct = total > 0 ? (count / total) * 100 : 0;
  return (
    <div>
      <div className="flex justify-between text-sm mb-1">
        <span className="text-brand-muted">{label}</span>
        <span className="font-medium">{count}</span>
      </div>
      <div className="h-1.5 bg-brand-border rounded-full overflow-hidden">
        <div
          className={`h-full ${color} rounded-full transition-all`}
          style={{ width: `${pct}%` }}
        />
      </div>
    </div>
  );
}
