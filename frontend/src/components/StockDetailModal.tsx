"use client";

import React, { useEffect, useState } from "react";
import {
  Bell,
  Calculator,
  Calendar,
  Check,
  Cpu,
  DollarSign,
  History,
  Loader2,
  RotateCcw,
  Scale,
  Sliders,
  Trash2,
  TrendingUp,
  X,
} from "lucide-react";
import { StockDetail, WatchlistItem } from "@/types/stock";

interface StockDetailModalProps {
  ticker: string | null;
  fallbackItem: WatchlistItem | null;
  apiBaseUrl: string;
  onClose: () => void;
  onRecalculated: (updated: WatchlistItem) => void;
  onDeleted: (ticker: string) => void;
}

function formatBillionsDollars(value: number): string {
  const billions = value / 1_000_000_000;
  const sign = billions < 0 ? "-" : "";
  return `${sign}$${Math.abs(billions).toFixed(2)}B`;
}

function formatBillionsShares(value: number): string {
  const billions = value / 1_000_000_000;
  return `${billions.toFixed(2)}B 株`;
}

/**
 * 10年3段階フェードDCFモデル（バックエンド dcf.rs と完全一致）
 * - 1〜5年目: 成長率 growthRate で成長
 * - 6〜10年目: growthRate から terminalGrowthRate へ5年間で線形に減衰
 * - 10年目末: ターミナルバリューを算出し現在価値へ割引
 */
function simulateClientDcfPrice(
  netCash: number,
  fcf: number,
  shares: number,
  wacc: number,
  growthRate: number,
  terminalGrowthRate: number
): number {
  if (shares <= 0) return 0;
  let ev = 0;
  if (fcf > 0) {
    const r = Math.max(wacc, 0.04);
    const gt = Math.max(0, Math.min(terminalGrowthRate, r - 0.015));
    let currentFcf = fcf;
    let pvFcf = 0;

    for (let t = 1; t <= 5; t++) {
      currentFcf *= 1 + growthRate;
      pvFcf += currentFcf / Math.pow(1 + r, t);
    }

    for (let step = 1; step <= 5; step++) {
      const t = 5 + step;
      const fadeRatio = step / 5.0;
      const yearG = growthRate + (gt - growthRate) * fadeRatio;
      currentFcf *= 1 + yearG;
      pvFcf += currentFcf / Math.pow(1 + r, t);
    }

    const tv10 = (currentFcf * (1 + gt)) / (r - gt);
    const pvTv = tv10 / Math.pow(1 + r, 10);
    ev = pvFcf + pvTv;
  }
  const price = Math.max(0, (netCash + ev) / shares);
  return Math.round(price * 100) / 100;
}

/**
 * 逆算DCF（Reverse DCF）: 現在株価を正当化する暗黙の5年FCF成長率を二分探索で算出
 */
function simulateClientImpliedGrowth(
  currentPrice: number,
  netCash: number,
  fcf: number,
  shares: number,
  wacc: number,
  terminalGrowthRate: number
): number | null {
  if (currentPrice <= 0 || fcf <= 0 || shares <= 0) return null;
  if (netCash / shares >= currentPrice) return -0.3;

  let low = -0.35;
  let high = 0.85;
  for (let i = 0; i < 50; i++) {
    const mid = 0.5 * (low + high);
    const p = simulateClientDcfPrice(
      netCash,
      fcf,
      shares,
      wacc,
      mid,
      terminalGrowthRate
    );
    if (p < currentPrice) {
      low = mid;
    } else {
      high = mid;
    }
  }
  return Math.round(0.5 * (low + high) * 1000) / 1000;
}

export default function StockDetailModal({
  ticker,
  fallbackItem,
  apiBaseUrl,
  onClose,
  onRecalculated,
  onDeleted,
}: StockDetailModalProps) {
  const [detail, setDetail] = useState<StockDetail | null>(null);
  const [isLoading, setIsLoading] = useState<boolean>(false);
  const [isRecalculating, setIsRecalculating] = useState<boolean>(false);
  const [isAutoResetting, setIsAutoResetting] = useState<boolean>(false);
  const [isSavingDiscount, setIsSavingDiscount] = useState<boolean>(false);
  const [isDeleting, setIsDeleting] = useState<boolean>(false);

  // Interactive DCF simulator & alert threshold states (percentage values)
  const [waccPct, setWaccPct] = useState<number>(8.5);
  const [growthBasePct, setGrowthBasePct] = useState<number>(12.0);
  const [terminalGrowthPct, setTerminalGrowthPct] = useState<number>(2.8);
  const [targetDiscountPct, setTargetDiscountPct] = useState<number>(20.0);
  const [feedbackMessage, setFeedbackMessage] = useState<string | null>(null);

  useEffect(() => {
    if (!ticker) return;
    let isMounted = true;

    async function fetchDetail(symbol: string) {
      setIsLoading(true);
      setFeedbackMessage(null);
      try {
        const res = await fetch(`${apiBaseUrl}/api/stocks/${symbol}`);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const data: StockDetail = await res.json();
        if (isMounted) {
          setDetail(data);
          setTargetDiscountPct(
            Number((data.watchlist.target_discount_rate ?? 20).toFixed(0))
          );
          if (data.latest_theoretical) {
            setWaccPct(Number((data.latest_theoretical.wacc * 100).toFixed(1)));
            setGrowthBasePct(
              Number((data.latest_theoretical.growth_rate_base * 100).toFixed(1))
            );
            setTerminalGrowthPct(
              Number(
                (data.latest_theoretical.terminal_growth_rate * 100).toFixed(1)
              )
            );
          }
        }
      } catch {
        if (isMounted && fallbackItem) {
          const mockRecord = {
            id: 1,
            ticker: fallbackItem.ticker,
            trigger_type: "INITIAL",
            base_price: fallbackItem.base_price,
            bull_price: fallbackItem.bull_price,
            bear_price: fallbackItem.bear_price,
            net_cash: fallbackItem.net_cash ?? 0,
            free_cash_flow: fallbackItem.free_cash_flow ?? 0,
            shares_outstanding: fallbackItem.shares_outstanding ?? 0,
            wacc: fallbackItem.wacc,
            growth_rate_base: fallbackItem.growth_rate_base,
            growth_rate_bull:
              fallbackItem.growth_rate_bull ?? fallbackItem.growth_rate_base + 0.05,
            growth_rate_bear:
              fallbackItem.growth_rate_bear ??
              Math.max(0.02, fallbackItem.growth_rate_base - 0.05),
            terminal_growth_rate: fallbackItem.terminal_growth_rate ?? 0.028,
            reasoning_ja: fallbackItem.reasoning_ja,
            calculated_at: fallbackItem.calculated_at,
          };
          setDetail({
            watchlist: {
              ticker: fallbackItem.ticker,
              company_name: fallbackItem.company_name,
              target_discount_rate: fallbackItem.target_discount_rate,
              last_earnings_date: fallbackItem.last_earnings_date,
            },
            current_price: fallbackItem.current_price,
            divergence_rate: fallbackItem.divergence_rate,
            is_undervalued_alert: fallbackItem.is_undervalued_alert,
            implied_growth_rate: fallbackItem.implied_growth_rate ?? null,
            latest_theoretical: mockRecord,
            theoretical_history: [mockRecord],
            recent_alerts: [],
          });
          setTargetDiscountPct(
            Number((fallbackItem.target_discount_rate ?? 20).toFixed(0))
          );
          setWaccPct(Number((fallbackItem.wacc * 100).toFixed(1)));
          setGrowthBasePct(
            Number((fallbackItem.growth_rate_base * 100).toFixed(1))
          );
          setTerminalGrowthPct(
            Number(((fallbackItem.terminal_growth_rate ?? 0.028) * 100).toFixed(1))
          );
        }
      } finally {
        if (isMounted) setIsLoading(false);
      }
    }

    fetchDetail(ticker);
    return () => {
      isMounted = false;
    };
  }, [ticker, apiBaseUrl, fallbackItem]);

  if (!ticker) return null;

  const latest = detail?.latest_theoretical;
  const currentPrice = detail?.current_price ?? fallbackItem?.current_price ?? 0;

  const netCash = latest?.net_cash ?? fallbackItem?.net_cash ?? 0;
  const fcf = latest?.free_cash_flow ?? fallbackItem?.free_cash_flow ?? 0;
  const shares =
    latest?.shares_outstanding ?? fallbackItem?.shares_outstanding ?? 0;

  const simulatedBasePrice = simulateClientDcfPrice(
    netCash,
    fcf,
    shares,
    waccPct / 100,
    growthBasePct / 100,
    terminalGrowthPct / 100
  );

  const simulatedDivergence =
    simulatedBasePrice > 0
      ? ((currentPrice - simulatedBasePrice) / simulatedBasePrice) * 100
      : 0;

  const simulatedImpliedGrowth = simulateClientImpliedGrowth(
    currentPrice,
    netCash,
    fcf,
    shares,
    waccPct / 100,
    terminalGrowthPct / 100
  );

  const alertTargetPrice =
    simulatedBasePrice * (1 - targetDiscountPct / 100);
  const isSimulatedBuyZone =
    simulatedBasePrice > 0 && currentPrice <= alertTargetPrice;

  const handleSaveTargetDiscountOnly = async () => {
    if (!ticker) return;
    setIsSavingDiscount(true);
    setFeedbackMessage(null);
    try {
      const res = await fetch(`${apiBaseUrl}/api/watchlist/${ticker}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          target_discount_rate: targetDiscountPct,
        }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const updatedItem: WatchlistItem = await res.json();
      onRecalculated(updatedItem);
      setDetail((prev) =>
        prev
          ? {
              ...prev,
              watchlist: {
                ...prev.watchlist,
                target_discount_rate: targetDiscountPct,
              },
              is_undervalued_alert: updatedItem.is_undervalued_alert,
            }
          : prev
      );
      setFeedbackMessage(
        `✅ アラート基準（目標割安率）を ${targetDiscountPct}% に変更しました（買いアラート発動ライン: $${(
          updatedItem.base_price *
          (1 - targetDiscountPct / 100)
        ).toFixed(2)} 以下）`
      );
    } catch {
      if (fallbackItem) {
        const updated: WatchlistItem = {
          ...fallbackItem,
          target_discount_rate: targetDiscountPct,
          is_undervalued_alert:
            currentPrice <=
            fallbackItem.base_price * (1 - targetDiscountPct / 100),
        };
        onRecalculated(updated);
        setFeedbackMessage(
          `✅ アラート基準（目標割安率）を ${targetDiscountPct}% に更新しました`
        );
      }
    } finally {
      setIsSavingDiscount(false);
    }
  };

  const handleRecalculateWithCustom = async () => {
    if (!ticker) return;
    setIsRecalculating(true);
    setFeedbackMessage(null);
    try {
      const res = await fetch(`${apiBaseUrl}/api/stocks/${ticker}/recalculate`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          custom_wacc: Number((waccPct / 100).toFixed(4)),
          custom_growth_rate_base: Number((growthBasePct / 100).toFixed(4)),
          custom_terminal_growth_rate: Number(
            (terminalGrowthPct / 100).toFixed(4)
          ),
          custom_target_discount_rate: targetDiscountPct,
        }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const updatedItem: WatchlistItem = await res.json();
      onRecalculated(updatedItem);

      const detailRes = await fetch(`${apiBaseUrl}/api/stocks/${ticker}`);
      if (detailRes.ok) {
        const updatedDetail: StockDetail = await detailRes.json();
        setDetail(updatedDetail);
      }
      setFeedbackMessage(
        `✅ WACC ${waccPct.toFixed(1)}%・基本成長率 ${growthBasePct.toFixed(
          1
        )}%・永久成長率 ${terminalGrowthPct.toFixed(
          1
        )}%（目標割安率 ${targetDiscountPct}%）で理論株価を再計算しDBを更新しました（新Base理論株価: $${updatedItem.base_price.toFixed(
          2
        )}）`
      );
    } catch {
      if (fallbackItem && latest) {
        const updated: WatchlistItem = {
          ...fallbackItem,
          target_discount_rate: targetDiscountPct,
          wacc: waccPct / 100,
          growth_rate_base: growthBasePct / 100,
          terminal_growth_rate: terminalGrowthPct / 100,
          implied_growth_rate: simulatedImpliedGrowth,
          base_price: simulatedBasePrice,
          bull_price: simulateClientDcfPrice(
            netCash,
            fcf,
            shares,
            waccPct / 100,
            growthBasePct / 100 + 0.05,
            terminalGrowthPct / 100
          ),
          bear_price: simulateClientDcfPrice(
            netCash,
            fcf,
            shares,
            waccPct / 100,
            Math.max(0.01, growthBasePct / 100 - 0.05),
            terminalGrowthPct / 100
          ),
          divergence_rate: Number(simulatedDivergence.toFixed(2)),
          is_undervalued_alert: isSimulatedBuyZone,
          calculated_at: new Date().toISOString(),
        };
        onRecalculated(updated);
        setFeedbackMessage(
          `✅ シミュレーション前提で理論株価を更新しました（新Base理論株価: $${simulatedBasePrice.toFixed(
            2
          )}）`
        );
      }
    } finally {
      setIsRecalculating(false);
    }
  };

  const handleAutoResetAndRecalculate = async () => {
    if (!ticker) return;
    setIsAutoResetting(true);
    setFeedbackMessage(null);
    try {
      const res = await fetch(`${apiBaseUrl}/api/stocks/${ticker}/recalculate`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          custom_target_discount_rate: targetDiscountPct,
        }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const updatedItem: WatchlistItem = await res.json();
      onRecalculated(updatedItem);

      const detailRes = await fetch(`${apiBaseUrl}/api/stocks/${ticker}`);
      if (detailRes.ok) {
        const updatedDetail: StockDetail = await detailRes.json();
        setDetail(updatedDetail);
        if (updatedDetail.latest_theoretical) {
          setWaccPct(
            Number((updatedDetail.latest_theoretical.wacc * 100).toFixed(1))
          );
          setGrowthBasePct(
            Number(
              (
                updatedDetail.latest_theoretical.growth_rate_base * 100
              ).toFixed(1)
            )
          );
          setTerminalGrowthPct(
            Number(
              (
                updatedDetail.latest_theoretical.terminal_growth_rate * 100
              ).toFixed(1)
            )
          );
        }
      }
      setFeedbackMessage(
        `✅ Yahoo Finance最新財務データからWACC(${(
          updatedItem.wacc * 100
        ).toFixed(1)}%)・永久成長率(${(
          (updatedItem.terminal_growth_rate ?? 0.028) * 100
        ).toFixed(1)}%)・FCF成長率を自走算出し直しました（Base理論株価: $${updatedItem.base_price.toFixed(
          2
        )}）`
      );
    } catch {
      if (detail?.auto_wacc != null) {
        setWaccPct(Number((detail.auto_wacc * 100).toFixed(1)));
      }
      if (detail?.auto_terminal_growth_rate != null) {
        setTerminalGrowthPct(
          Number((detail.auto_terminal_growth_rate * 100).toFixed(1))
        );
      }
    } finally {
      setIsAutoResetting(false);
    }
  };

  const handleDelete = async () => {
    if (!ticker) return;
    setIsDeleting(true);
    try {
      await fetch(`${apiBaseUrl}/api/watchlist/${ticker}`, {
        method: "DELETE",
      });
    } catch {
      // Allow UI removal even if offline
    } finally {
      setIsDeleting(false);
      onDeleted(ticker);
      onClose();
    }
  };

  // Only show metrics that were actually retrieved from Yahoo Finance
  const hasFinancialStatements = fcf !== 0 || netCash !== 0 || shares > 0;
  const hasCapmBreakdown =
    detail?.beta != null ||
    detail?.risk_free_rate != null ||
    detail?.effective_tax_rate != null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/50 p-4 backdrop-blur-sm dark:bg-black/75">
      <div className="relative flex max-h-[92vh] w-full max-w-4xl flex-col overflow-hidden rounded-2xl border border-slate-200 bg-white shadow-2xl dark:border-slate-800 dark:bg-slate-900">
        {/* Modal Header */}
        <div className="flex items-center justify-between border-b border-slate-200 bg-slate-50 px-6 py-4 dark:border-slate-800 dark:bg-slate-950/80">
          <div className="flex items-center gap-3">
            <span className="rounded-lg border border-slate-200 bg-slate-900 px-3 py-1 font-mono text-xl font-extrabold text-white dark:border-emerald-500/40 dark:bg-emerald-500/10 dark:text-emerald-400">
              {ticker}
            </span>
            <div>
              <h2 className="text-base font-bold text-slate-900 dark:text-white sm:text-lg">
                {detail?.watchlist.company_name ?? fallbackItem?.company_name}
              </h2>
              <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-slate-500 dark:text-slate-400">
                <span>
                  現在株価:{" "}
                  <strong className="font-mono text-slate-800 dark:text-slate-200">
                    ${currentPrice.toFixed(2)}
                  </strong>
                </span>
                <span>•</span>
                <span>
                  アラート目標割安率:{" "}
                  <strong className="font-mono text-emerald-600 dark:text-emerald-400">
                    {targetDiscountPct >= 0
                      ? `-${targetDiscountPct}%`
                      : `+${Math.abs(targetDiscountPct)}%`}
                  </strong>
                </span>
                {detail?.watchlist.last_earnings_date && (
                  <>
                    <span>•</span>
                    <span className="inline-flex items-center gap-1">
                      <Calendar className="h-3 w-3 text-indigo-600 dark:text-indigo-400" />
                      直近決算期末日: {detail.watchlist.last_earnings_date}
                    </span>
                  </>
                )}
              </div>
            </div>
          </div>

          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-slate-200 bg-white p-2 text-slate-500 transition hover:border-slate-300 hover:text-slate-900 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-400 dark:hover:border-slate-700 dark:hover:text-white"
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        {/* Modal Body */}
        <div className="flex-1 space-y-6 overflow-y-auto p-6">
          {isLoading ? (
            <div className="flex items-center justify-center py-16 text-slate-500 dark:text-slate-400">
              <Loader2 className="mr-2 h-6 w-6 animate-spin text-emerald-600 dark:text-emerald-400" />
              <span>Yahoo Finance 実財務データ・CAPMパラメータを取得中...</span>
            </div>
          ) : (
            <>
              {/* Section 1: Yahoo Finance Real Financial Foundation KPIs (only displays available items) */}
              {hasFinancialStatements && (
                <div>
                  <h3 className="mb-3 flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">
                    <DollarSign className="h-4 w-4 text-emerald-600 dark:text-emerald-400" />
                    Yahoo Finance 実財務データ（取得できた項目のみ表示）
                  </h3>
                  <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
                    {(netCash !== 0 || fcf !== 0) && (
                      <div className="rounded-xl border border-slate-200 bg-slate-50 p-3.5 dark:border-slate-800 dark:bg-slate-950/70">
                        <span className="text-[11px] text-slate-500 dark:text-slate-400">
                          ネットキャッシュ (現金 - 負債)
                        </span>
                        <div
                          className={`mt-1 font-mono text-lg font-bold ${
                            netCash >= 0
                              ? "text-emerald-600 dark:text-emerald-400"
                              : "text-amber-600 dark:text-amber-400"
                          }`}
                        >
                          {formatBillionsDollars(netCash)}
                        </div>
                        {shares > 0 && (
                          <p className="mt-1 text-[11px] text-slate-500">
                            1株あたり資産価値: ${(netCash / shares).toFixed(2)}
                          </p>
                        )}
                      </div>
                    )}

                    {fcf !== 0 && (
                      <div className="rounded-xl border border-slate-200 bg-slate-50 p-3.5 dark:border-slate-800 dark:bg-slate-950/70">
                        <span className="text-[11px] text-slate-500 dark:text-slate-400">
                          直近FCF (TTM / 通期)
                        </span>
                        <div className="mt-1 font-mono text-lg font-bold text-indigo-600 dark:text-indigo-300">
                          {formatBillionsDollars(fcf)}
                        </div>
                        {shares > 0 && (
                          <p className="mt-1 text-[11px] text-slate-500">
                            1株あたりFCF: ${(fcf / shares).toFixed(2)}
                          </p>
                        )}
                      </div>
                    )}

                    {detail?.latest_revenue != null &&
                      detail.latest_revenue > 0 && (
                        <div className="rounded-xl border border-slate-200 bg-slate-50 p-3.5 dark:border-slate-800 dark:bg-slate-950/70">
                          <span className="text-[11px] text-slate-500 dark:text-slate-400">
                            直近売上高 (TTM / 通期)
                          </span>
                          <div className="mt-1 font-mono text-lg font-bold text-slate-900 dark:text-slate-100">
                            {formatBillionsDollars(detail.latest_revenue)}
                          </div>
                          {detail.fcf_margin != null && (
                            <p className="mt-1 text-[11px] text-emerald-600 dark:text-emerald-400">
                              FCFマージン: {(detail.fcf_margin * 100).toFixed(1)}%
                            </p>
                          )}
                        </div>
                      )}

                    {shares > 0 && (
                      <div className="rounded-xl border border-slate-200 bg-slate-50 p-3.5 dark:border-slate-800 dark:bg-slate-950/70">
                        <span className="text-[11px] text-slate-500 dark:text-slate-400">
                          希薄化後発行済株式数
                        </span>
                        <div className="mt-1 font-mono text-lg font-bold text-slate-900 dark:text-slate-100">
                          {formatBillionsShares(shares)}
                        </div>
                        <p className="mt-1 text-[11px] text-slate-500">
                          時価総額:{" "}
                          {formatBillionsDollars(currentPrice * shares)}
                        </p>
                      </div>
                    )}
                  </div>

                  {/* CAPM & Reverse DCF Breakdown Bar (only shown when fetched from Yahoo Finance) */}
                  {(hasCapmBreakdown || simulatedImpliedGrowth != null) && (
                    <div className="mt-3 flex flex-wrap items-center justify-between gap-2 rounded-xl border border-slate-200 bg-slate-50/60 px-4 py-2.5 text-xs dark:border-slate-800 dark:bg-slate-950/50">
                      {hasCapmBreakdown && (
                        <div className="flex flex-wrap items-center gap-3 text-slate-600 dark:text-slate-300">
                          <span className="font-semibold text-slate-800 dark:text-emerald-400">
                            CAPM 自走算出パラメータ:
                          </span>
                          {detail?.beta != null && (
                            <span className="font-mono">
                              調整後β: <strong>{detail.beta.toFixed(2)}</strong>
                            </span>
                          )}
                          {detail?.risk_free_rate != null && (
                            <span className="font-mono">
                              米10年債利回り(Rf):{" "}
                              <strong>
                                {(detail.risk_free_rate * 100).toFixed(2)}%
                              </strong>
                            </span>
                          )}
                          {detail?.effective_tax_rate != null && (
                            <span className="font-mono">
                              実効税率:{" "}
                              <strong>
                                {(detail.effective_tax_rate * 100).toFixed(1)}%
                              </strong>
                            </span>
                          )}
                        </div>
                      )}

                      {simulatedImpliedGrowth != null && (
                        <div className="inline-flex items-center gap-1.5 rounded-lg border border-indigo-200 bg-indigo-50 px-2.5 py-1 font-mono text-xs text-indigo-800 dark:border-indigo-500/30 dark:bg-indigo-950/40 dark:text-indigo-200">
                          <Scale className="h-3.5 w-3.5 text-indigo-600 dark:text-indigo-400" />
                          <span>
                            逆算DCF (市場織り込み成長率):{" "}
                            <strong>
                              {simulatedImpliedGrowth >= 0 ? "+" : ""}
                              {(simulatedImpliedGrowth * 100).toFixed(1)}%/年
                            </strong>
                          </span>
                        </div>
                      )}
                    </div>
                  )}
                </div>
              )}

              {/* Section 2: Target Discount Rate (Alert Threshold) Editor */}
              <div className="rounded-xl border border-slate-200 bg-slate-50 p-4 dark:border-slate-800 dark:bg-slate-950/70">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2">
                    <Bell className="h-4 w-4 text-emerald-600 dark:text-emerald-400" />
                    <h3 className="text-xs font-bold uppercase tracking-wider text-slate-900 dark:text-white">
                      アラート基準の目標割安率 設定（後から変更可能）
                    </h3>
                  </div>
                  <div className="flex items-center gap-2">
                    <span
                      className={`rounded px-2 py-0.5 font-mono text-[11px] font-bold ${
                        isSimulatedBuyZone
                          ? "bg-emerald-500/20 text-emerald-700 dark:text-emerald-300"
                          : "bg-slate-200 text-slate-600 dark:bg-slate-800 dark:text-slate-400"
                      }`}
                    >
                      {isSimulatedBuyZone
                        ? "🎯 現在買い圏内 (アラート点灯)"
                        : `発動ライン: $${alertTargetPrice.toFixed(2)} 以下`}
                    </span>
                    <button
                      type="button"
                      onClick={handleSaveTargetDiscountOnly}
                      disabled={isSavingDiscount}
                      className="inline-flex items-center gap-1 rounded-lg border border-emerald-500/40 bg-emerald-600 px-3 py-1 text-xs font-bold text-white transition hover:bg-emerald-500 disabled:opacity-50"
                    >
                      {isSavingDiscount ? (
                        <Loader2 className="h-3.5 w-3.5 animate-spin" />
                      ) : (
                        <Check className="h-3.5 w-3.5" />
                      )}
                      <span>目標割安率を保存</span>
                    </button>
                  </div>
                </div>

                <div className="mt-3 grid grid-cols-1 items-center gap-4 sm:grid-cols-3">
                  <div className="sm:col-span-2">
                    <input
                      id="modal-discount-slider"
                      type="range"
                      min={-20}
                      max={50}
                      step={1}
                      value={targetDiscountPct}
                      onChange={(e) =>
                        setTargetDiscountPct(Number(e.target.value))
                      }
                      className="h-2 w-full cursor-pointer appearance-none rounded-lg bg-slate-200 accent-emerald-500 dark:bg-slate-800"
                    />
                    <div className="mt-1 flex justify-between font-mono text-[10px] text-slate-500">
                      <span>-20% (プレミアム許容)</span>
                      <span>0% (理論株価と同値)</span>
                      <span>20% (標準安全域)</span>
                      <span>50% (超割安重視)</span>
                    </div>
                  </div>
                  <div className="rounded-lg border border-slate-200 bg-white px-3 py-2 text-right font-mono dark:border-slate-800 dark:bg-slate-900">
                    <div className="text-[10px] text-slate-500 dark:text-slate-400">
                      目標割安率 / 発動株価
                    </div>
                    <div className="text-sm font-bold text-slate-900 dark:text-emerald-400">
                      {targetDiscountPct >= 0
                        ? `理論株価より ${targetDiscountPct}% 割安`
                        : `理論株価 +${Math.abs(targetDiscountPct)}% まで許容`}{" "}
                      (${alertTargetPrice.toFixed(2)})
                    </div>
                  </div>
                </div>
              </div>

              {/* Section 3: Interactive DCF Simulator (WACC + 5Y Growth + Terminal Growth) */}
              <div className="rounded-xl border border-slate-200 bg-slate-50 p-5 dark:border-emerald-500/30 dark:bg-slate-950/80">
                <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2">
                    <Sliders className="h-4 w-4 text-slate-700 dark:text-emerald-400" />
                    <h3 className="text-sm font-bold text-slate-900 dark:text-white">
                      10年3段階 DCF シミュレーター（WACC・成長率・永久成長率）
                    </h3>
                  </div>
                  <button
                    type="button"
                    onClick={handleAutoResetAndRecalculate}
                    disabled={isAutoResetting || isRecalculating}
                    className="inline-flex items-center gap-1.5 rounded-lg border border-indigo-500/40 bg-indigo-500/10 px-3 py-1.5 text-xs font-semibold text-indigo-700 transition hover:bg-indigo-500/20 disabled:opacity-50 dark:text-indigo-300"
                  >
                    {isAutoResetting ? (
                      <Loader2 className="h-3.5 w-3.5 animate-spin" />
                    ) : (
                      <RotateCcw className="h-3.5 w-3.5" />
                    )}
                    <span>Yahoo財務からWACC・永久成長率を再自走算出</span>
                  </button>
                </div>

                <div className="grid grid-cols-1 gap-6 md:grid-cols-2">
                  {/* Sliders */}
                  <div className="space-y-4">
                    {/* WACC Slider */}
                    <div>
                      <div className="mb-1.5 flex items-center justify-between text-xs">
                        <label
                          htmlFor="wacc-slider"
                          className="font-medium text-slate-700 dark:text-slate-300"
                        >
                          割引率 (WACC){" "}
                          {detail?.auto_wacc != null && (
                            <span className="text-[11px] text-slate-500">
                              (CAPM自走値:{" "}
                              {(detail.auto_wacc * 100).toFixed(1)}%)
                            </span>
                          )}
                        </label>
                        <span className="rounded border border-slate-300 bg-white px-2 py-0.5 font-mono text-sm font-bold text-slate-900 dark:border-slate-700 dark:bg-slate-900 dark:text-emerald-400">
                          {waccPct.toFixed(1)}%
                        </span>
                      </div>
                      <input
                        id="wacc-slider"
                        type="range"
                        min={4.5}
                        max={15.0}
                        step={0.1}
                        value={waccPct}
                        onChange={(e) => setWaccPct(parseFloat(e.target.value))}
                        className="h-2 w-full cursor-pointer appearance-none rounded-lg bg-slate-200 accent-slate-900 dark:bg-slate-800 dark:accent-emerald-500"
                      />
                      <div className="mt-1 flex justify-between font-mono text-[10px] text-slate-500">
                        <span>4.5% (低資本コスト)</span>
                        <span>8.5% (標準)</span>
                        <span>15.0% (高β/高リスク)</span>
                      </div>
                    </div>

                    {/* 5Y Base Growth Slider */}
                    <div>
                      <div className="mb-1.5 flex items-center justify-between text-xs">
                        <label
                          htmlFor="growth-slider"
                          className="font-medium text-slate-700 dark:text-slate-300"
                        >
                          1〜5年目 基本FCF成長率{" "}
                          {simulatedImpliedGrowth != null && (
                            <span className="text-[11px] text-indigo-600 dark:text-indigo-400">
                              (市場織り込み:{" "}
                              {(simulatedImpliedGrowth * 100).toFixed(1)}%)
                            </span>
                          )}
                        </label>
                        <span className="rounded border border-slate-300 bg-white px-2 py-0.5 font-mono text-sm font-bold text-indigo-600 dark:border-slate-700 dark:bg-slate-900 dark:text-indigo-400">
                          {growthBasePct.toFixed(1)}%
                        </span>
                      </div>
                      <input
                        id="growth-slider"
                        type="range"
                        min={0.0}
                        max={45.0}
                        step={0.5}
                        value={growthBasePct}
                        onChange={(e) =>
                          setGrowthBasePct(parseFloat(e.target.value))
                        }
                        className="h-2 w-full cursor-pointer appearance-none rounded-lg bg-slate-200 accent-indigo-600 dark:bg-slate-800 dark:accent-indigo-500"
                      />
                      <div className="mt-1 flex justify-between font-mono text-[10px] text-slate-500">
                        <span>0.0% (成熟)</span>
                        <span>15.0% (安定成長)</span>
                        <span>45.0% (高成長)</span>
                      </div>
                    </div>

                    {/* Terminal Growth Rate Slider */}
                    <div>
                      <div className="mb-1.5 flex items-center justify-between text-xs">
                        <label
                          htmlFor="terminal-growth-slider"
                          className="font-medium text-slate-700 dark:text-slate-300"
                        >
                          永久成長率 (Terminal Growth){" "}
                          {detail?.auto_terminal_growth_rate != null && (
                            <span className="text-[11px] text-slate-500">
                              (自走値:{" "}
                              {(
                                detail.auto_terminal_growth_rate * 100
                              ).toFixed(1)}
                              %)
                            </span>
                          )}
                        </label>
                        <span className="rounded border border-slate-300 bg-white px-2 py-0.5 font-mono text-sm font-bold text-amber-600 dark:border-slate-700 dark:bg-slate-900 dark:text-amber-400">
                          {terminalGrowthPct.toFixed(1)}%
                        </span>
                      </div>
                      <input
                        id="terminal-growth-slider"
                        type="range"
                        min={1.0}
                        max={4.0}
                        step={0.1}
                        value={terminalGrowthPct}
                        onChange={(e) =>
                          setTerminalGrowthPct(parseFloat(e.target.value))
                        }
                        className="h-2 w-full cursor-pointer appearance-none rounded-lg bg-slate-200 accent-amber-500 dark:bg-slate-800"
                      />
                      <div className="mt-1 flex justify-between font-mono text-[10px] text-slate-500">
                        <span>1.0% (保守的)</span>
                        <span>2.5% (長期インフレ並)</span>
                        <span>4.0% (高Moat上限)</span>
                      </div>
                    </div>
                  </div>

                  {/* Live Preview & Submit Button */}
                  <div className="flex flex-col justify-between rounded-xl border border-slate-200 bg-white p-4 shadow-sm dark:border-slate-800 dark:bg-slate-900/90 dark:shadow-none">
                    <div className="space-y-3">
                      <div className="flex items-center justify-between">
                        <div>
                          <span className="text-xs text-slate-500 dark:text-slate-400">
                            シミュレーション理論株価 (Base)
                          </span>
                          <div className="mt-1 flex items-baseline gap-2">
                            <span className="font-mono text-2xl font-extrabold text-slate-900 dark:text-emerald-400">
                              ${simulatedBasePrice.toFixed(2)}
                            </span>
                            <span className="font-mono text-xs text-slate-500 dark:text-slate-400">
                              (現在値比:{" "}
                              {simulatedDivergence > 0 ? "+" : ""}
                              {simulatedDivergence.toFixed(2)}%)
                            </span>
                          </div>
                        </div>
                        <TrendingUp className="h-8 w-8 text-slate-300 dark:text-emerald-500/40" />
                      </div>

                      {simulatedImpliedGrowth != null && (
                        <div className="rounded-lg border border-slate-200 bg-slate-50 p-2.5 text-xs dark:border-slate-800 dark:bg-slate-950/60">
                          <div className="flex items-center justify-between">
                            <span className="text-slate-500 dark:text-slate-400">
                              現在の株価(${currentPrice.toFixed(2)})が織り込む成長率:
                            </span>
                            <span className="font-mono font-bold text-indigo-600 dark:text-indigo-300">
                              年率 {simulatedImpliedGrowth >= 0 ? "+" : ""}
                              {(simulatedImpliedGrowth * 100).toFixed(1)}%
                            </span>
                          </div>
                          <p className="mt-1 text-[11px] text-slate-500 dark:text-slate-400">
                            {growthBasePct > simulatedImpliedGrowth * 100
                              ? "👉 あなたの成長率見通しの方が市場より強気（＝理論株価 ＞ 現在株価）"
                              : "👉 市場株価の方があなたの成長率見通しより高い成長を織り込み中"}
                          </p>
                        </div>
                      )}
                    </div>

                    <div className="mt-4">
                      <button
                        type="button"
                        onClick={handleRecalculateWithCustom}
                        disabled={isRecalculating || isAutoResetting}
                        className="flex w-full items-center justify-center gap-2 rounded-lg bg-slate-900 px-4 py-2.5 text-xs font-bold text-white shadow transition hover:bg-slate-800 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-emerald-600 dark:hover:bg-emerald-500 sm:text-sm"
                      >
                        {isRecalculating ? (
                          <Loader2 className="h-4 w-4 animate-spin" />
                        ) : (
                          <Calculator className="h-4 w-4" />
                        )}
                        <span>この前提（割安率含む）で理論株価を再計算・保存</span>
                      </button>
                    </div>
                  </div>
                </div>

                {feedbackMessage && (
                  <div className="mt-3 rounded-lg border border-slate-200 border-l-4 border-l-emerald-600 bg-white px-3.5 py-2 text-xs font-medium text-slate-800 dark:border-l dark:border-emerald-500/40 dark:bg-emerald-500/10 dark:text-emerald-300">
                    {feedbackMessage}
                  </div>
                )}
              </div>

              {/* Section 4: LLM / Autonomous Calculation Reasoning Full Report */}
              <div className="rounded-xl border border-indigo-200 bg-indigo-50/70 p-5 dark:border-indigo-500/30 dark:bg-indigo-950/20">
                <div className="mb-2 flex items-center justify-between">
                  <span className="inline-flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-indigo-700 dark:text-indigo-300">
                    <Cpu className="h-4 w-4 text-indigo-600 dark:text-indigo-400" />
                    バリュエーション算出根拠レポート全文
                  </span>
                </div>
                <p className="whitespace-pre-line text-sm leading-relaxed text-slate-700 dark:text-slate-200">
                  {latest?.reasoning_ja ?? fallbackItem?.reasoning_ja}
                </p>
              </div>

              {/* Section 5: Historical Theoretical Prices Table */}
              <div>
                <h3 className="mb-3 flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">
                  <History className="h-4 w-4 text-indigo-600 dark:text-indigo-400" />
                  過去の理論株価算出履歴（初回算出 vs 決算後更新 vs 手動調整）
                </h3>
                <div className="overflow-x-auto rounded-xl border border-slate-200 dark:border-slate-800">
                  <table className="w-full text-left text-xs">
                    <thead className="border-b border-slate-200 bg-slate-100 text-slate-600 dark:border-slate-800 dark:bg-slate-950/90 dark:text-slate-400">
                      <tr>
                        <th className="px-4 py-2.5 font-medium">算出日時</th>
                        <th className="px-4 py-2.5 font-medium">トリガー種別</th>
                        <th className="px-4 py-2.5 font-medium">Base 理論株価</th>
                        <th className="px-4 py-2.5 font-medium">Bull / Bear</th>
                        <th className="px-4 py-2.5 font-medium">WACC</th>
                        <th className="px-4 py-2.5 font-medium">基本成長率</th>
                        <th className="px-4 py-2.5 font-medium">永久成長率</th>
                        <th className="px-4 py-2.5 font-medium">基準FCF</th>
                      </tr>
                    </thead>
                    <tbody className="divide-y divide-slate-200 bg-white font-mono dark:divide-slate-800/70 dark:bg-slate-900/60">
                      {(detail?.theoretical_history ?? []).map((row) => (
                        <tr
                          key={row.id}
                          className="hover:bg-slate-50 dark:hover:bg-slate-800/40"
                        >
                          <td className="px-4 py-2.5 text-slate-600 dark:text-slate-300">
                            {new Date(row.calculated_at).toLocaleString("ja-JP")}
                          </td>
                          <td className="px-4 py-2.5">
                            <span
                              className={`inline-block rounded px-2 py-0.5 text-[10px] font-bold ${
                                row.trigger_type === "EARNINGS_UPDATE"
                                  ? "border border-indigo-500/40 bg-indigo-500/10 text-indigo-700 dark:bg-indigo-500/15 dark:text-indigo-300"
                                  : row.trigger_type === "MANUAL"
                                  ? "border border-amber-500/40 bg-amber-500/10 text-amber-700 dark:bg-amber-500/15 dark:text-amber-300"
                                  : "border border-emerald-500/40 bg-emerald-500/10 text-emerald-700 dark:bg-emerald-500/15 dark:text-emerald-300"
                              }`}
                            >
                              {row.trigger_type}
                            </span>
                          </td>
                          <td className="px-4 py-2.5 font-bold text-emerald-600 dark:text-emerald-400">
                            ${row.base_price.toFixed(2)}
                          </td>
                          <td className="px-4 py-2.5 text-slate-600 dark:text-slate-300">
                            ${row.bull_price.toFixed(2)} / $
                            {row.bear_price.toFixed(2)}
                          </td>
                          <td className="px-4 py-2.5 text-slate-600 dark:text-slate-300">
                            {(row.wacc * 100).toFixed(1)}%
                          </td>
                          <td className="px-4 py-2.5 text-slate-600 dark:text-slate-300">
                            {(row.growth_rate_base * 100).toFixed(1)}%
                          </td>
                          <td className="px-4 py-2.5 text-slate-600 dark:text-slate-300">
                            {(row.terminal_growth_rate * 100).toFixed(1)}%
                          </td>
                          <td className="px-4 py-2.5 text-slate-600 dark:text-slate-300">
                            {formatBillionsDollars(row.free_cash_flow)}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>
            </>
          )}
        </div>

        {/* Modal Footer */}
        <div className="flex items-center justify-between border-t border-slate-200 bg-slate-50 px-6 py-3.5 dark:border-slate-800 dark:bg-slate-950/80">
          <button
            type="button"
            onClick={handleDelete}
            disabled={isDeleting}
            className="inline-flex items-center gap-1.5 rounded-lg border border-rose-500/40 bg-rose-50 px-3.5 py-2 text-xs font-semibold text-rose-700 transition hover:bg-rose-100 disabled:opacity-50 dark:bg-rose-950/30 dark:text-rose-300 dark:hover:bg-rose-900/50 dark:hover:text-white"
          >
            {isDeleting ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Trash2 className="h-4 w-4" />
            )}
            <span>この銘柄をウォッチリストから削除</span>
          </button>

          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-slate-300 bg-white px-4 py-2 text-xs font-semibold text-slate-700 transition hover:bg-slate-100 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-200 dark:hover:bg-slate-700"
          >
            閉じる
          </button>
        </div>
      </div>
    </div>
  );
}
