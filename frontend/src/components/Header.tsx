"use client";

import React from "react";
import {
  Activity,
  BarChart3,
  Cpu,
  Loader2,
  Plus,
  RefreshCw,
} from "lucide-react";

interface HeaderProps {
  onOpenAddModal: () => void;
  onRunDailyCheck: () => Promise<void>;
  onRunEarningsCheck: () => Promise<void>;
  isDailyLoading: boolean;
  isEarningsLoading: boolean;
  llmModel?: string | null;
}

export default function Header({
  onOpenAddModal,
  onRunDailyCheck,
  onRunEarningsCheck,
  isDailyLoading,
  isEarningsLoading,
  llmModel,
}: HeaderProps) {
  const isAnyJobRunning = isDailyLoading || isEarningsLoading;

  return (
    <header className="sticky top-0 z-30 border-b border-slate-800/80 bg-slate-950/90 backdrop-blur-md">
      <div className="mx-auto flex max-w-[1440px] flex-col gap-4 px-4 py-3.5 sm:px-6 lg:flex-row lg:items-center lg:justify-between">
        {/* Left: Brand Logo & Sub-badge */}
        <div className="flex items-center gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-lg border border-emerald-500/40 bg-emerald-500/10 text-emerald-400 shadow-inner shadow-emerald-500/20">
            <Activity className="h-5 w-5" />
          </div>
          <div>
            <div className="flex flex-wrap items-center gap-2.5">
              <h1 className="text-lg font-bold tracking-tight text-slate-100 sm:text-xl">
                Personal Stock Analyst
              </h1>
              <span className="inline-flex items-center gap-1.5 rounded-full border border-indigo-500/40 bg-indigo-500/10 px-2.5 py-0.5 text-xs font-medium text-indigo-300">
                <Cpu className="h-3.5 w-3.5 text-indigo-400" />
                Powered by {llmModel || "LLM"}
              </span>
            </div>
            <p className="text-xs text-slate-400">
              Institutional DCF Valuation &amp; Automated LINE Equity Alert Terminal
            </p>
          </div>
        </div>

        {/* Right: Action Buttons */}
        <div className="flex flex-wrap items-center gap-2.5">
          <button
            type="button"
            onClick={onOpenAddModal}
            disabled={isAnyJobRunning}
            className="inline-flex items-center gap-1.5 rounded-lg bg-emerald-600 px-3.5 py-2 text-xs font-semibold text-white shadow-sm shadow-emerald-950 transition hover:bg-emerald-500 disabled:cursor-not-allowed disabled:opacity-50 sm:text-sm"
          >
            <Plus className="h-4 w-4" />
            <span>銘柄を追加</span>
          </button>

          <button
            type="button"
            onClick={onRunDailyCheck}
            disabled={isAnyJobRunning}
            className="inline-flex items-center gap-1.5 rounded-lg border border-slate-700 bg-slate-900 px-3.5 py-2 text-xs font-medium text-slate-200 transition hover:border-emerald-500/50 hover:bg-slate-800 hover:text-emerald-300 disabled:cursor-not-allowed disabled:opacity-50 sm:text-sm"
          >
            {isDailyLoading ? (
              <Loader2 className="h-4 w-4 animate-spin text-emerald-400" />
            ) : (
              <RefreshCw className="h-4 w-4 text-emerald-400" />
            )}
            <span>🔄 日次監視＆ニュース要約を実行</span>
          </button>

          <button
            type="button"
            onClick={onRunEarningsCheck}
            disabled={isAnyJobRunning}
            className="inline-flex items-center gap-1.5 rounded-lg border border-indigo-500/40 bg-indigo-950/40 px-3.5 py-2 text-xs font-medium text-indigo-200 transition hover:border-indigo-400 hover:bg-indigo-900/50 hover:text-white disabled:cursor-not-allowed disabled:opacity-50 sm:text-sm"
          >
            {isEarningsLoading ? (
              <Loader2 className="h-4 w-4 animate-spin text-indigo-400" />
            ) : (
              <BarChart3 className="h-4 w-4 text-indigo-400" />
            )}
            <span>📊 決算チェック＆再計算を実行</span>
          </button>
        </div>
      </div>
    </header>
  );
}
