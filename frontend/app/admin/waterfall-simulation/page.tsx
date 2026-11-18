'use client';

import { useState } from 'react';
import { TrancheClass } from '@/../packages/sdk/src/generated/tranche';

interface WaterfallResult {
  seniorAmount: bigint;
  juniorAmount: bigint;
  seniorCap: bigint;
  elapsedYield: bigint;
}

export default function WaterfallSimulationPage() {
  const [invoiceId, setInvoiceId] = useState<number>(1);
  const [totalDue, setTotalDue] = useState<number>(1000);
  const [seniorPrincipal, setSeniorPrincipal] = useState<number>(800);
  const [juniorPrincipal, setJuniorPrincipal] = useState<number>(200);
  const [seniorTargetYieldBps, setSeniorTargetYieldBps] = useState<number>(1000);
  const [elapsedSecs, setElapsedSecs] = useState<number>(30 * 24 * 60 * 60); // 30 days
  const [result, setResult] = useState<WaterfallResult | null>(null);

  const simulateWaterfall = () => {
    // Calculate senior cap: principal + time-proportional yield
    const seniorCap =
      seniorPrincipal +
      (seniorPrincipal * seniorTargetYieldBps * elapsedSecs) / (365 * 24 * 60 * 60 * 10000);

    // Calculate elapsed yield portion
    const elapsedYield =
      (seniorPrincipal * seniorTargetYieldBps * elapsedSecs) / (365 * 24 * 60 * 60 * 10000);

    // Senior gets min(totalDue, cap)
    const seniorAmount = Math.min(totalDue, seniorCap);

    // Junior gets remainder
    const juniorAmount = Math.max(0, totalDue - seniorAmount);

    setResult({
      seniorAmount: BigInt(Math.round(seniorAmount * 1e7)), // Convert to contract units
      juniorAmount: BigInt(Math.round(juniorAmount * 1e7)),
      seniorCap: BigInt(Math.round(seniorCap * 1e7)),
      elapsedYield: BigInt(Math.round(elapsedYield * 1e7)),
    });
  };

  const simulateLossAllocation = () => {
    const shortfall = Math.max(0, seniorPrincipal + juniorPrincipal - totalDue);
    const juniorRemaining = juniorPrincipal;

    const juniorLoss = Math.min(shortfall, juniorRemaining);
    const seniorLoss = Math.max(0, shortfall - juniorRemaining);

    return { juniorLoss, seniorLoss, shortfall };
  };

  const lossResult = simulateLossAllocation();

  return (
    <div className="min-h-screen bg-gray-50">
      <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 py-8">
        {/* Header */}
        <div className="mb-8">
          <h1 className="text-3xl font-bold text-gray-900">Waterfall Simulation</h1>
          <p className="mt-2 text-gray-600">
            Simulate waterfall repayment and loss allocation for hypothetical scenarios
          </p>
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Input Parameters */}
          <div className="bg-white rounded-lg shadow-md p-6 border border-gray-200">
            <h2 className="text-xl font-semibold text-gray-900 mb-6">Simulation Parameters</h2>

            <div className="space-y-4">
              {/* Invoice ID */}
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-2">Invoice ID</label>
                <input
                  type="number"
                  value={invoiceId}
                  onChange={(e) => setInvoiceId(Number(e.target.value))}
                  className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                />
              </div>

              {/* Total Due */}
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-2">
                  Total Repayment Amount ($)
                </label>
                <input
                  type="number"
                  value={totalDue}
                  onChange={(e) => setTotalDue(Number(e.target.value))}
                  className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                  placeholder="Enter repayment amount"
                />
                <p className="mt-1 text-xs text-gray-500">Principal + interest being repaid</p>
              </div>

              {/* Senior Principal */}
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-2">
                  Senior Principal ($)
                </label>
                <input
                  type="number"
                  value={seniorPrincipal}
                  onChange={(e) => setSeniorPrincipal(Number(e.target.value))}
                  className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                />
                <p className="mt-1 text-xs text-gray-500">Amount funded by senior tranche</p>
              </div>

              {/* Junior Principal */}
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-2">
                  Junior Principal ($)
                </label>
                <input
                  type="number"
                  value={juniorPrincipal}
                  onChange={(e) => setJuniorPrincipal(Number(e.target.value))}
                  className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                />
                <p className="mt-1 text-xs text-gray-500">Amount funded by junior tranche</p>
              </div>

              {/* Senior Target Yield */}
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-2">
                  Senior Target Yield (BPS)
                </label>
                <input
                  type="number"
                  value={seniorTargetYieldBps}
                  onChange={(e) => setSeniorTargetYieldBps(Number(e.target.value))}
                  className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                />
                <p className="mt-1 text-xs text-gray-500">
                  {(seniorTargetYieldBps / 100).toFixed(1)}% annual target yield
                </p>
              </div>

              {/* Elapsed Time */}
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-2">
                  Elapsed Time (seconds)
                </label>
                <input
                  type="number"
                  value={elapsedSecs}
                  onChange={(e) => setElapsedSecs(Number(e.target.value))}
                  className="block w-full rounded-md border-gray-300 shadow-sm focus:border-blue-500 focus:ring-blue-500"
                />
                <p className="mt-1 text-xs text-gray-500">
                  {(elapsedSecs / (24 * 60 * 60)).toFixed(1)} days elapsed
                </p>
              </div>

              <button
                onClick={simulateWaterfall}
                className="w-full bg-blue-600 text-white py-3 px-4 rounded-lg font-medium hover:bg-blue-700"
              >
                Run Simulation
              </button>
            </div>
          </div>

          {/* Results */}
          <div className="space-y-6">
            {/* Waterfall Results */}
            <div className="bg-white rounded-lg shadow-md p-6 border border-gray-200">
              <h2 className="text-xl font-semibold text-gray-900 mb-6">Waterfall Distribution</h2>

              {result ? (
                <div className="space-y-4">
                  {/* Senior Payout */}
                  <div className="bg-blue-50 rounded-lg p-4 border border-blue-200">
                    <h3 className="font-medium text-blue-900 mb-2">Senior Tranche</h3>
                    <div className="space-y-2 text-sm">
                      <div className="flex justify-between">
                        <span className="text-blue-700">Payout</span>
                        <span className="font-medium text-blue-900">
                          ${(Number(result.seniorAmount) / 1e7).toLocaleString()}
                        </span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-blue-700">Cap</span>
                        <span className="font-medium text-blue-900">
                          ${(Number(result.seniorCap) / 1e7).toLocaleString()}
                        </span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-blue-700">Elapsed Yield</span>
                        <span className="font-medium text-green-600">
                          ${(Number(result.elapsedYield) / 1e7).toLocaleString()}
                        </span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-blue-700">Principal Return</span>
                        <span className="font-medium text-blue-900">
                          ${seniorPrincipal.toLocaleString()}
                        </span>
                      </div>
                    </div>
                  </div>

                  {/* Junior Payout */}
                  <div className="bg-purple-50 rounded-lg p-4 border border-purple-200">
                    <h3 className="font-medium text-purple-900 mb-2">Junior Tranche</h3>
                    <div className="space-y-2 text-sm">
                      <div className="flex justify-between">
                        <span className="text-purple-700">Payout</span>
                        <span className="font-medium text-purple-900">
                          ${(Number(result.juniorAmount) / 1e7).toLocaleString()}
                        </span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-purple-700">Principal Return</span>
                        <span className="font-medium text-purple-900">
                          $
                          {Math.min(
                            juniorPrincipal,
                            Number(result.juniorAmount) / 1e7,
                          ).toLocaleString()}
                        </span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-purple-700">Residual Yield</span>
                        <span className="font-medium text-green-600">
                          $
                          {Math.max(
                            0,
                            Number(result.juniorAmount) / 1e7 - juniorPrincipal,
                          ).toLocaleString()}
                        </span>
                      </div>
                    </div>
                  </div>

                  {/* Summary */}
                  <div className="bg-gray-50 rounded-lg p-4 border border-gray-200">
                    <div className="space-y-2 text-sm">
                      <div className="flex justify-between">
                        <span className="text-gray-600">Total Distributed</span>
                        <span className="font-medium text-gray-900">
                          $
                          {(
                            (Number(result.seniorAmount) + Number(result.juniorAmount)) /
                            1e7
                          ).toLocaleString()}
                        </span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-gray-600">Total Due</span>
                        <span className="font-medium text-gray-900">
                          ${totalDue.toLocaleString()}
                        </span>
                      </div>
                      <div className="flex justify-between">
                        <span className="text-gray-600">Remaining</span>
                        <span className="font-medium text-gray-900">
                          $
                          {Math.max(
                            0,
                            totalDue -
                              (Number(result.seniorAmount) + Number(result.juniorAmount)) / 1e7,
                          ).toLocaleString()}
                        </span>
                      </div>
                    </div>
                  </div>
                </div>
              ) : (
                <div className="text-center py-8 text-gray-500">Run simulation to see results</div>
              )}
            </div>

            {/* Loss Allocation Results */}
            <div className="bg-white rounded-lg shadow-md p-6 border border-gray-200">
              <h2 className="text-xl font-semibold text-gray-900 mb-6">
                Loss Allocation (if Default)
              </h2>

              <div className="space-y-4">
                {lossResult.shortfall > 0 ? (
                  <>
                    <div className="bg-red-50 rounded-lg p-4 border border-red-200">
                      <h3 className="font-medium text-red-900 mb-2">Shortfall</h3>
                      <p className="text-2xl font-bold text-red-700">
                        ${lossResult.shortfall.toLocaleString()}
                      </p>
                      <p className="text-xs text-red-600 mt-1">
                        Principal owed: ${(seniorPrincipal + juniorPrincipal).toLocaleString()}
                      </p>
                    </div>

                    <div className="bg-purple-50 rounded-lg p-4 border border-purple-200">
                      <h3 className="font-medium text-purple-900 mb-2">Junior Absorbs First</h3>
                      <div className="space-y-2 text-sm">
                        <div className="flex justify-between">
                          <span className="text-purple-700">Junior Loss</span>
                          <span className="font-medium text-red-600">
                            ${lossResult.juniorLoss.toLocaleString()}
                          </span>
                        </div>
                        <div className="flex justify-between">
                          <span className="text-purple-700">Junior Remaining</span>
                          <span className="font-medium text-purple-900">
                            ${(juniorPrincipal - lossResult.juniorLoss).toLocaleString()}
                          </span>
                        </div>
                      </div>
                    </div>

                    {lossResult.seniorLoss > 0 && (
                      <div className="bg-blue-50 rounded-lg p-4 border border-blue-200">
                        <h3 className="font-medium text-blue-900 mb-2">Senior Takes Remaining</h3>
                        <div className="space-y-2 text-sm">
                          <div className="flex justify-between">
                            <span className="text-blue-700">Senior Loss</span>
                            <span className="font-medium text-red-600">
                              ${lossResult.seniorLoss.toLocaleString()}
                            </span>
                          </div>
                          <div className="flex justify-between">
                            <span className="text-blue-700">Senior Remaining</span>
                            <span className="font-medium text-blue-900">
                              ${(seniorPrincipal - lossResult.seniorLoss).toLocaleString()}
                            </span>
                          </div>
                        </div>
                      </div>
                    )}

                    {lossResult.seniorLoss === 0 && (
                      <div className="bg-green-50 rounded-lg p-4 border border-green-200">
                        <p className="text-sm text-green-800">
                          <strong>Senior Protected:</strong> Junior capital absorbed the entire
                          shortfall. Senior investors remain whole.
                        </p>
                      </div>
                    )}
                  </>
                ) : (
                  <div className="bg-green-50 rounded-lg p-4 border border-green-200">
                    <p className="text-sm text-green-800">
                      <strong>No Default:</strong> Repayment covers full principal. No loss
                      allocation needed.
                    </p>
                  </div>
                )}
              </div>
            </div>
          </div>
        </div>

        {/* Scenario Presets */}
        <div className="mt-8 bg-white rounded-lg shadow-md p-6 border border-gray-200">
          <h2 className="text-xl font-semibold text-gray-900 mb-4">Common Scenarios</h2>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
            <button
              onClick={() => {
                setTotalDue(1100);
                setSeniorPrincipal(800);
                setJuniorPrincipal(200);
                setElapsedSecs(30 * 24 * 60 * 60);
              }}
              className="p-4 border border-gray-200 rounded-lg hover:bg-gray-50 text-left"
            >
              <h3 className="font-medium text-gray-900">Full Repayment</h3>
              <p className="text-xs text-gray-500 mt-1">110% of principal, 30 days elapsed</p>
            </button>
            <button
              onClick={() => {
                setTotalDue(500);
                setSeniorPrincipal(800);
                setJuniorPrincipal(200);
                setElapsedSecs(15 * 24 * 60 * 60);
              }}
              className="p-4 border border-gray-200 rounded-lg hover:bg-gray-50 text-left"
            >
              <h3 className="font-medium text-gray-900">Partial Default</h3>
              <p className="text-xs text-gray-500 mt-1">50% recovery, 15 days elapsed</p>
            </button>
            <button
              onClick={() => {
                setTotalDue(0);
                setSeniorPrincipal(800);
                setJuniorPrincipal(200);
                setElapsedSecs(60 * 24 * 60 * 60);
              }}
              className="p-4 border border-gray-200 rounded-lg hover:bg-gray-50 text-left"
            >
              <h3 className="font-medium text-gray-900">Total Default</h3>
              <p className="text-xs text-gray-500 mt-1">Zero recovery, 60 days elapsed</p>
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
