"use client";

import React, { useEffect, useState } from "react";
import {
  Calculator,
  Calendar,
  Cpu,
  DollarSign,
  History,
  Loader2,
  Sliders,
  Sparkles,
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
  return `${sign}$${Math.abs(billions).toFixed(1)}B`;
}

function formatBillionsShares(value: number): string {
  const billions = value / 1_000_000_000;
  return `${billions.toFixed(2)}B 株`;
}

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
    let pvFcf = 0;
    for (let t = 1; t <= 5; t++) {
      pvFcf += (fcf * Math.pow(1 + growthRate, t)) / Math.pow(1 + r, t);
    }
    const fcf5 = fcf * Math.pow(1 + growthRate, 5);
    const tv = (fcf5 * (1 + gt)) / (r - gt);
    const pvTv = tv / Math.pow(1 + r, 5);
    ev = pvFcf + pvTv;
  }
  const price = Math.max(0, (netCash + ev) / shares);
  return Math.round(price * 100) / 100;
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
  const [isDeleting, setIsDeleting] = useState<boolean>(false);

  // Interactive DCF simulator state (percentage values, e.g., 8.5 for 8.5%)
  const [waccPct, setWaccPct] = useState<number>(8.5);
  const [growthBasePct, setGrowthBasePct] = useState<number>(12.0);
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
          if (data.latest_theoretical) {
            setWaccPct(Number((data.latest_theoretical.wacc * 100).toFixed(1)));
            setGrowthBasePct(
              Number((data.latest_theoretical.growth_rate_base * 100).toFixed(1))
            );
          }
        }
      } catch {
        // Fallback construction from WatchlistItem if backend is offline
        if (isMounted && fallbackItem) {
          const mockRecord = {
            id: 1,
            ticker: fallbackItem.ticker,
            trigger_type: "INITIAL",
            base_price: fallbackItem.base_price,
            bull_price: fallbackItem.bull_price,
            bear_price: fallbackItem.bear_price,
            net_cash: fallbackItem.net_cash ?? -41_400_000_000,
            free_cash_flow: fallbackItem.free_cash_flow ?? 108_800_000_000,
            shares_outstanding: fallbackItem.shares_outstanding ?? 15_200_000_000,
            wacc: fallbackItem.wacc,
            growth_rate_base: fallbackItem.growth_rate_base,
            growth_rate_bull:
              fallbackItem.growth_rate_bull ?? fallbackItem.growth_rate_base + 0.05,
            growth_rate_bear:
              fallbackItem.growth_rate_bear ??
              Math.max(0.02, fallbackItem.growth_rate_base - 0.05),
            terminal_growth_rate: fallbackItem.terminal_growth_rate ?? 0.025,
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
            latest_theoretical: mockRecord,
            theoretical_history: [mockRecord],
            recent_alerts: [],
          });
          setWaccPct(Number((fallbackItem.wacc * 100).toFixed(1)));
          setGrowthBasePct(
            Number((fallbackItem.growth_rate_base * 100).toFixed(1))
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
    latest?.shares_outstanding ?? fallbackItem?.shares_outstanding ?? 1;
  const terminalG = latest?.terminal_growth_rate ?? 0.025;

  const simulatedBasePrice = simulateClientDcfPrice(
    netCash,
    fcf,
    shares,
    waccPct / 100,
    growthBasePct / 100,
    terminalG
  );

  const simulatedDivergence =
    simulatedBasePrice > 0
      ? ((currentPrice - simulatedBasePrice) / simulatedBasePrice) * 100
      : 0;

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
        }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const updatedItem: WatchlistItem = await res.json();
      onRecalculated(updatedItem);

      // Refresh modal detail history
      const detailRes = await fetch(`${apiBaseUrl}/api/stocks/${ticker}`);
      if (detailRes.ok) {
        const updatedDetail: StockDetail = await detailRes.json();
        setDetail(updatedDetail);
      }
      setFeedbackMessage(
        `✅ WACC ${waccPct.toFixed(1)}%・基本成長率 ${growthBasePct.toFixed(
          1
        )}% で理論株価を再計算しDBを更新しました（新Base理論株価: $${updatedItem.base_price.toFixed(
          2
        )}）`
      );
    } catch {
      if (fallbackItem && latest) {
        const updated: WatchlistItem = {
          ...fallbackItem,
          wacc: waccPct / 100,
          growth_rate_base: growthBasePct / 100,
          base_price: simulatedBasePrice,
          bull_price: simulateClientDcfPrice(
            netCash,
            fcf,
            shares,
            waccPct / 100,
            growthBasePct / 100 + 0.05,
            terminalG
          ),
          bear_price: simulateClientDcfPrice(
            netCash,
            fcf,
            shares,
            waccPct / 100,
            Math.max(0.01, growthBasePct / 100 - 0.05),
            terminalG
          ),
          divergence_rate: Number(simulatedDivergence.toFixed(2)),
          is_undervalued_alert:
            currentPrice <=
            simulatedBasePrice *
              (1 - (detail?.watchlist.target_discount_rate ?? 20) / 100),
          calculated_at: new Date().toISOString(),
        };
        onRecalculated(updated);
        setFeedbackMessage(
          `✅ ローカルシミュレーションで理論株価を更新しました（新Base理論株価: $${simulatedBasePrice.toFixed(
            2
          )}）`
        );
      }
    } finally {
      setIsRecalculating(false);
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

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-slate-900/50 p-4 backdrop-blur-sm dark:bg-black/75">
      <div className="relative flex max-h-[90vh] w-full max-w-4xl flex-col overflow-hidden rounded-2xl border border-slate-200 bg-white shadow-2xl dark:border-slate-800 dark:bg-slate-900">
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
              <div className="flex items-center gap-3 text-xs text-slate-500 dark:text-slate-400">
                <span>
                  現在株価:{" "}
                  <strong className="font-mono text-slate-800 dark:text-slate-200">
                    ${currentPrice.toFixed(2)}
                  </strong>
                </span>
                <span>•</span>
                <span>
                  目標割安率:{" "}
                  <strong className="font-mono text-slate-800 dark:text-emerald-400">
                    -
                    {(
                      detail?.watchlist.target_discount_rate ??
                      fallbackItem?.target_discount_rate ??
                      20
                    ).toFixed(0)}
                    %
                  </strong>
                </span>
                {detail?.watchlist.last_earnings_date && (
                  <>
                    <span>•</span>
                    <span className="inline-flex items-center gap-1">
                      <Calendar className="h-3 w-3 text-indigo-600 dark:text-indigo-400" />
                      直近決算日: {detail.watchlist.last_earnings_date}
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
              <span>財務・DCF履歴データを読み込み中...</span>
            </div>
          ) : (
            <>
              {/* Section 1: Financial Foundation KPIs ($XX.XB notation) */}
              <div>
                <h3 className="mb-3 flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-slate-500 dark:text-slate-400">
                  <DollarSign className="h-4 w-4 text-emerald-600 dark:text-emerald-400" />
                  財務基礎データ（資産価値 ＋ 事業価値ベース）
                </h3>
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
                  <div className="rounded-xl border border-slate-200 bg-slate-50 p-4 dark:border-slate-800 dark:bg-slate-950/70">
                    <span className="text-xs text-slate-500 dark:text-slate-400">
                      ネットキャッシュ (現金 - 有利子負債)
                    </span>
                    <div
                      className={`mt-1 font-mono text-xl font-bold ${
                        netCash >= 0
                          ? "text-emerald-600 dark:text-emerald-400"
                          : "text-amber-600 dark:text-amber-400"
                      }`}
                    >
                      {formatBillionsDollars(netCash)}
                    </div>
                    <p className="mt-1 text-[11px] text-slate-500">
                      1株あたり資産価値:{" "}
                      {shares > 0 ? `$${(netCash / shares).toFixed(2)}` : "$0.00"}
                    </p>
                  </div>

                  <div className="rounded-xl border border-slate-200 bg-slate-50 p-4 dark:border-slate-800 dark:bg-slate-950/70">
                    <span className="text-xs text-slate-500 dark:text-slate-400">
                      直近通期フリーキャッシュフロー (FCF)
                    </span>
                    <div className="mt-1 font-mono text-xl font-bold text-indigo-600 dark:text-indigo-300">
                      {formatBillionsDollars(fcf)}
                    </div>
                    <p className="mt-1 text-[11px] text-slate-500">
                      1株あたりFCF:{" "}
                      {shares > 0 ? `$${(fcf / shares).toFixed(2)}` : "$0.00"}
                    </p>
                  </div>

                  <div className="rounded-xl border border-slate-200 bg-slate-50 p-4 dark:border-slate-800 dark:bg-slate-950/70">
                    <span className="text-xs text-slate-500 dark:text-slate-400">
                      希薄化後発行済株式数
                    </span>
                    <div className="mt-1 font-mono text-xl font-bold text-slate-900 dark:text-slate-100">
                      {formatBillionsShares(shares)}
                    </div>
                    <p className="mt-1 text-[11px] text-slate-500">
                      永久成長率前提: {(terminalG * 100).toFixed(1)}%
                    </p>
                  </div>
                </div>
              </div>

              {/* Section 2: LLM Reasoning Full Report */}
              <div className="rounded-xl border border-indigo-200 bg-indigo-50/70 p-5 dark:border-indigo-500/30 dark:bg-indigo-950/20">
                <div className="mb-2 flex items-center justify-between">
                  <span className="inline-flex items-center gap-2 text-xs font-bold uppercase tracking-wider text-indigo-700 dark:text-indigo-300">
                    <Cpu className="h-4 w-4 text-indigo-600 dark:text-indigo-400" />
                    LLM 算出根拠レポート全文
                  </span>
                </div>
                <p className="whitespace-pre-line text-sm leading-relaxed text-slate-700 dark:text-slate-200">
                  {latest?.reasoning_ja ?? fallbackItem?.reasoning_ja}
                </p>
              </div>

              {/* Section 3: Interactive DCF Simulator */}
              <div className="rounded-xl border border-slate-200 bg-slate-50 p-5 dark:border-emerald-500/30 dark:bg-slate-950/80">
                <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
                  <div className="flex items-center gap-2">
                    <Sliders className="h-4 w-4 text-slate-700 dark:text-emerald-400" />
                    <h3 className="text-sm font-bold text-slate-900 dark:text-white">
                      インタラクティブ DCF シミュレーター
                    </h3>
                  </div>
                  <span className="text-xs text-slate-500 dark:text-slate-400">
                    スライダーで割引率(WACC)と成長率を調整して理論株価を再計算
                  </span>
                </div>

                <div className="grid grid-cols-1 gap-6 md:grid-cols-2">
                  {/* Sliders */}
                  <div className="space-y-4">
                    <div>
                      <div className="mb-1.5 flex items-center justify-between text-xs">
                        <label
                          htmlFor="wacc-slider"
                          className="font-medium text-slate-700 dark:text-slate-300"
                        >
                          割引率 (WACC)
                        </label>
                        <span className="rounded border border-slate-300 bg-white px-2 py-0.5 font-mono text-sm font-bold text-slate-900 dark:border-slate-700 dark:bg-slate-900 dark:text-emerald-400">
                          {waccPct.toFixed(1)}%
                        </span>
                      </div>
                      <input
                        id="wacc-slider"
                        type="range"
                        min={4.0}
                        max={16.0}
                        step={0.1}
                        value={waccPct}
                        onChange={(e) => setWaccPct(parseFloat(e.target.value))}
                        className="h-2 w-full cursor-pointer appearance-none rounded-lg bg-slate-200 accent-slate-900 dark:bg-slate-800 dark:accent-emerald-500"
                      />
                      <div className="mt-1 flex justify-between font-mono text-[10px] text-slate-500">
                        <span>4.0% (低金利)</span>
                        <span>8.5% (標準)</span>
                        <span>16.0% (高リスク)</span>
                      </div>
                    </div>

                    <div>
                      <div className="mb-1.5 flex items-center justify-between text-xs">
                        <label
                          htmlFor="growth-slider"
                          className="font-medium text-slate-700 dark:text-slate-300"
                        >
                          今後5年間の基本FCF成長率 (Base Growth)
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
                        <span>45.0% (急成長)</span>
                      </div>
                    </div>
                  </div>

                  {/* Live Preview & Submit Button */}
                  <div className="flex flex-col justify-between rounded-xl border border-slate-200 bg-white p-4 shadow-sm dark:border-slate-800 dark:bg-slate-900/90 dark:shadow-none">
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

                    <div className="mt-4">
                      <button
                        type="button"
                        onClick={handleRecalculateWithCustom}
                        disabled={isRecalculating}
                        className="flex w-full items-center justify-center gap-2 rounded-lg bg-slate-900 px-4 py-2.5 text-xs font-bold text-white shadow transition hover:bg-slate-800 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-emerald-600 dark:hover:bg-emerald-500 sm:text-sm"
                      >
                        {isRecalculating ? (
                          <Loader2 className="h-4 w-4 animate-spin" />
                        ) : (
                          <Calculator className="h-4 w-4" />
                        )}
                        <span>この前提で理論株価を再計算</span>
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

              {/* Section 4: Historical Theoretical Prices Table */}
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
