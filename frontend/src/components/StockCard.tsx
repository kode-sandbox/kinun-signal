"use client";

import React from "react";
import {
  ArrowDownRight,
  ArrowUpRight,
  ChevronRight,
  Sparkles,
  Target,
} from "lucide-react";
import { WatchlistItem } from "@/types/stock";

interface StockCardProps {
  stock: WatchlistItem;
  onSelect: (ticker: string) => void;
}

export default function StockCard({ stock, onSelect }: StockCardProps) {
  const maxBarPrice = Math.max(
    stock.bull_price,
    stock.base_price,
    stock.bear_price,
    stock.current_price,
    1
  );

  const bullWidth = Math.min(100, Math.max(12, (stock.bull_price / maxBarPrice) * 100));
  const baseWidth = Math.min(100, Math.max(12, (stock.base_price / maxBarPrice) * 100));
  const bearWidth = Math.min(100, Math.max(12, (stock.bear_price / maxBarPrice) * 100));

  const isDiscount = stock.divergence_rate < 0;
  const reasoningPreview =
    stock.reasoning_ja.length > 100
      ? `${stock.reasoning_ja.slice(0, 100)}...`
      : stock.reasoning_ja;

  return (
    <div
      onClick={() => onSelect(stock.ticker)}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect(stock.ticker);
        }
      }}
      className={`group relative flex flex-col justify-between rounded-xl border p-5 transition-all duration-200 cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-indigo-500 ${
        stock.is_undervalued_alert
          ? "border-slate-300 border-l-4 border-l-emerald-600 bg-white shadow-sm hover:border-slate-400 hover:border-l-emerald-600 hover:shadow-md active:bg-slate-50/70 dark:border-l dark:border-emerald-500/90 dark:bg-slate-900/95 dark:shadow-[0_0_25px_-5px_rgba(16,185,129,0.35)] dark:hover:border-emerald-400 dark:active:bg-slate-900"
          : "border-slate-200 bg-white shadow-sm hover:border-slate-300 hover:shadow-md active:bg-slate-50/70 dark:border-slate-800 dark:bg-slate-900/90 dark:shadow-none dark:hover:border-slate-700 dark:hover:bg-slate-900"
      }`}
    >
      {/* Top Row: Ticker, Company & Alert / Divergence Badges */}
      <div>
        <div className="flex items-start justify-between gap-2">
          <div>
            <div className="flex items-center gap-2">
              <span className="font-mono text-xl font-extrabold tracking-tight text-slate-900 dark:text-white">
                {stock.ticker}
              </span>
              <span className="rounded border border-slate-200 bg-slate-100 px-1.5 py-0.5 font-mono text-[11px] text-slate-600 dark:border-slate-700 dark:bg-slate-800/80 dark:text-slate-300">
                目標割安率 -{stock.target_discount_rate.toFixed(0)}%
              </span>
            </div>
            <p className="mt-0.5 text-xs font-medium text-slate-500 dark:text-slate-400 line-clamp-1">
              {stock.company_name}
            </p>
          </div>

          {/* Divergence Badge */}
          <div
            className={`inline-flex items-center gap-1 rounded-md border px-2.5 py-1 font-mono text-xs font-bold ${
              isDiscount
                ? "border-slate-200 bg-slate-100 text-emerald-700 dark:border-emerald-500/40 dark:bg-emerald-500/15 dark:text-emerald-300"
                : "border-amber-200 bg-amber-50 text-amber-700 dark:border-amber-500/40 dark:bg-amber-500/15 dark:text-amber-300"
            }`}
          >
            {isDiscount ? (
              <ArrowDownRight className="h-3.5 w-3.5 text-emerald-600 dark:text-emerald-300" />
            ) : (
              <ArrowUpRight className="h-3.5 w-3.5" />
            )}
            <span>
              {stock.divergence_rate > 0 ? "+" : ""}
              {stock.divergence_rate.toFixed(2)}%
            </span>
          </div>
        </div>

        {/* Undervalued Target Reached Badge */}
        {stock.is_undervalued_alert && (
          <div className="mt-3 flex items-center justify-between rounded-lg border border-slate-200 bg-slate-900 px-3 py-1.5 text-xs font-semibold text-white shadow-sm dark:border-emerald-500/50 dark:bg-emerald-500/15 dark:text-emerald-300 dark:shadow-none">
            <div className="flex items-center gap-1.5">
              <Target className="h-4 w-4 shrink-0 text-emerald-400" />
              <span>🎯 買い圏内 (目標割安率達成)</span>
            </div>
            <span className="rounded bg-emerald-500 px-1.5 py-0.5 font-mono text-[10px] font-bold text-slate-950 dark:bg-emerald-500/20 dark:text-emerald-300">
              BUY ZONE
            </span>
          </div>
        )}

        {/* Price Comparison Header */}
        <div className="mt-4 grid grid-cols-2 gap-3 rounded-lg border border-slate-200 bg-slate-50/80 p-3 dark:border-slate-800/90 dark:bg-slate-950/60">
          <div>
            <span className="text-[11px] font-medium uppercase tracking-wider text-slate-500 dark:text-slate-400">
              現在株価 (Close)
            </span>
            <div className="mt-0.5 font-mono text-xl font-bold text-slate-900 dark:text-slate-100">
              ${stock.current_price.toFixed(2)}
            </div>
          </div>
          <div className="border-l border-slate-200 pl-3 dark:border-slate-800">
            <span className="text-[11px] font-medium uppercase tracking-wider text-indigo-600 dark:text-indigo-300">
              理論株価 (Base DCF)
            </span>
            <div className="mt-0.5 font-mono text-xl font-bold text-indigo-600 dark:text-emerald-400">
              ${stock.base_price.toFixed(2)}
            </div>
          </div>
        </div>

        {/* 3-Bar Scenario Visualization (Bull / Base / Bear) */}
        <div className="mt-4 space-y-2">
          <div className="flex items-center justify-between text-[11px] text-slate-500 dark:text-slate-400">
            <span>DCF シナリオレンジ</span>
            <span className="font-mono text-slate-500 dark:text-slate-400">
              WACC {(stock.wacc * 100).toFixed(1)}% | 成長率{" "}
              {(stock.growth_rate_base * 100).toFixed(1)}%
            </span>
          </div>

          {/* Bull Bar */}
          <div className="space-y-1">
            <div className="flex justify-between font-mono text-xs">
              <span className="text-slate-600 dark:text-indigo-300">Bull (強気)</span>
              <span className="font-semibold text-slate-900 dark:text-indigo-200">
                ${stock.bull_price.toFixed(2)}
              </span>
            </div>
            <div className="h-2 w-full overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
              <div
                className="h-full rounded-full bg-indigo-500 dark:bg-gradient-to-r dark:from-indigo-500 dark:to-indigo-400"
                style={{ width: `${bullWidth}%` }}
              />
            </div>
          </div>

          {/* Base Bar */}
          <div className="space-y-1">
            <div className="flex justify-between font-mono text-xs">
              <span className="font-medium text-slate-700 dark:text-emerald-300">Base (基本)</span>
              <span className="font-bold text-slate-900 dark:text-emerald-200">
                ${stock.base_price.toFixed(2)}
              </span>
            </div>
            <div className="h-2 w-full overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
              <div
                className="h-full rounded-full bg-slate-800 dark:bg-gradient-to-r dark:from-emerald-500 dark:to-emerald-400"
                style={{ width: `${baseWidth}%` }}
              />
            </div>
          </div>

          {/* Bear Bar */}
          <div className="space-y-1">
            <div className="flex justify-between font-mono text-xs">
              <span className="text-slate-500 dark:text-slate-400">Bear (弱気)</span>
              <span className="font-semibold text-slate-600 dark:text-slate-300">
                ${stock.bear_price.toFixed(2)}
              </span>
            </div>
            <div className="h-2 w-full overflow-hidden rounded-full bg-slate-100 dark:bg-slate-800">
              <div
                className="h-full rounded-full bg-slate-400 dark:bg-slate-500"
                style={{ width: `${bearWidth}%` }}
              />
            </div>
          </div>
        </div>
      </div>

      {/* Bottom: LLM Reasoning 100-char Preview */}
      <div className="mt-4 border-t border-slate-100 pt-3 dark:border-slate-800/80">
        <div className="mb-1 flex items-center justify-between text-[11px] font-medium text-indigo-600 dark:text-indigo-300">
          <span className="inline-flex items-center gap-1">
            <Sparkles className="h-3 w-3 text-indigo-500 dark:text-indigo-400" />
            算出根拠プレビュー
          </span>
          <span className="inline-flex items-center text-slate-500 group-hover:text-indigo-600 dark:text-slate-400 dark:group-hover:text-emerald-400">
            詳細・DCF再計算
            <ChevronRight className="h-3.5 w-3.5 transition-transform group-hover:translate-x-0.5" />
          </span>
        </div>
        <p className="text-xs leading-relaxed text-slate-600 dark:text-slate-300">
          {reasoningPreview}
        </p>
      </div>
    </div>
  );
}
