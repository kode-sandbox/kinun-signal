"use client";

import React from "react";
import {
  Activity,
  BarChart3,
  Cpu,
  Loader2,
  Moon,
  Plus,
  RefreshCw,
  Sun,
} from "lucide-react";

interface HeaderProps {
  onOpenAddModal: () => void;
  onRunDailyCheck: () => Promise<void>;
  onRunEarningsCheck: () => Promise<void>;
  isDailyLoading: boolean;
  isEarningsLoading: boolean;
  theme?: "dark" | "light";
  onToggleTheme?: () => void;
  llmModel?: string | null;
}

export default function Header({
  onOpenAddModal,
  onRunDailyCheck,
  onRunEarningsCheck,
  isDailyLoading,
  isEarningsLoading,
  theme = "dark",
  onToggleTheme,
  llmModel,
}: HeaderProps) {
  const isAnyJobRunning = isDailyLoading || isEarningsLoading;
  const isDark = theme === "dark";

  return (
    <header className="sticky top-0 z-30 border-b border-slate-200 bg-white/90 backdrop-blur-md transition-colors duration-200 dark:border-slate-800/80 dark:bg-slate-950/90">
      <div className="mx-auto flex max-w-[1440px] flex-col gap-4 px-4 py-3.5 sm:px-6 lg:flex-row lg:items-center lg:justify-between">
        {/* Left: Brand Logo & Sub-badge */}
        <div className="flex items-center gap-3">
          <div className="flex h-10 w-10 items-center justify-center rounded-lg border border-slate-200 bg-slate-900 text-white shadow-sm dark:border-emerald-500/40 dark:bg-emerald-500/10 dark:text-emerald-400 dark:shadow-inner dark:shadow-emerald-500/20">
            <Activity className="h-5 w-5" />
          </div>
          <div>
            <div className="flex flex-wrap items-center gap-2.5">
              <h1 className="text-lg font-bold tracking-tight text-slate-900 dark:text-slate-100 sm:text-xl">
                Personal Stock Analyst
              </h1>
              <span className="inline-flex items-center gap-1.5 rounded-full border border-slate-200 bg-slate-100 px-2.5 py-0.5 text-xs font-semibold text-slate-700 dark:border-indigo-500/40 dark:bg-indigo-500/10 dark:text-indigo-300">
                <Cpu className="h-3.5 w-3.5 text-indigo-600 dark:text-indigo-400" />
                Powered by argon LLM
              <span className="inline-flex items-center gap-1.5 rounded-full border border-indigo-500/40 bg-indigo-500/10 px-2.5 py-0.5 text-xs font-medium text-indigo-300">
                <Cpu className="h-3.5 w-3.5 text-indigo-400" />
                Powered by {llmModel || "LLM"}
              </span>
            </div>
            <p className="text-xs text-slate-500 dark:text-slate-400">
              Institutional DCF Valuation &amp; Automated LINE Equity Alert Terminal
            </p>
          </div>
        </div>

        {/* Right: Theme Switcher & Action Buttons */}
        <div className="flex flex-wrap items-center gap-2.5">
          {onToggleTheme && (
            <button
              type="button"
              onClick={onToggleTheme}
              title={
                isDark
                  ? "ホワイトベース（ライトモード）に切り替え"
                  : "ブラックベース（ダークモード）に切り替え"
              }
              aria-label="画面テーマ切り替え"
              className="inline-flex items-center gap-1.5 rounded-lg border border-slate-300 bg-white px-3 py-2 text-xs font-semibold text-slate-700 shadow-sm transition hover:bg-slate-50 hover:text-slate-900 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-200 dark:shadow-none dark:hover:border-slate-600 dark:hover:bg-slate-800 sm:text-sm"
            >
              {isDark ? (
                <>
                  <Sun className="h-4 w-4 text-amber-400" />
                  <span>ホワイト画面へ切替</span>
                </>
              ) : (
                <>
                  <Moon className="h-4 w-4 text-slate-700" />
                  <span>ブラック画面へ切替</span>
                </>
              )}
            </button>
          )}

          <button
            type="button"
            onClick={onOpenAddModal}
            disabled={isAnyJobRunning}
            className="inline-flex items-center gap-1.5 rounded-lg bg-slate-900 px-3.5 py-2 text-xs font-semibold text-white shadow-sm transition hover:bg-slate-800 disabled:cursor-not-allowed disabled:opacity-50 dark:bg-emerald-600 dark:shadow-emerald-950 dark:hover:bg-emerald-500 sm:text-sm"
          >
            <Plus className="h-4 w-4" />
            <span>銘柄を追加</span>
          </button>

          <button
            type="button"
            onClick={onRunDailyCheck}
            disabled={isAnyJobRunning}
            className="inline-flex items-center gap-1.5 rounded-lg border border-slate-300 bg-white px-3.5 py-2 text-xs font-medium text-slate-700 shadow-sm transition hover:border-slate-400 hover:bg-slate-50 hover:text-slate-900 disabled:cursor-not-allowed disabled:opacity-50 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-200 dark:shadow-none dark:hover:border-emerald-500/50 dark:hover:bg-slate-800 dark:hover:text-emerald-300 sm:text-sm"
          >
            {isDailyLoading ? (
              <Loader2 className="h-4 w-4 animate-spin text-slate-700 dark:text-emerald-400" />
            ) : (
              <RefreshCw className="h-4 w-4 text-slate-600 dark:text-emerald-400" />
            )}
            <span>🔄 日次監視＆ニュース要約を実行</span>
          </button>

          <button
            type="button"
            onClick={onRunEarningsCheck}
            disabled={isAnyJobRunning}
            className="inline-flex items-center gap-1.5 rounded-lg border border-indigo-200 bg-indigo-600 px-3.5 py-2 text-xs font-medium text-white shadow-sm transition hover:bg-indigo-500 disabled:cursor-not-allowed disabled:opacity-50 dark:border-indigo-500/40 dark:bg-indigo-950/40 dark:text-indigo-200 dark:shadow-none dark:hover:border-indigo-400 dark:hover:bg-indigo-900/50 dark:hover:text-white sm:text-sm"
          >
            {isEarningsLoading ? (
              <Loader2 className="h-4 w-4 animate-spin text-white dark:text-indigo-400" />
            ) : (
              <BarChart3 className="h-4 w-4 text-white dark:text-indigo-400" />
            )}
            <span>📊 決算チェック＆再計算を実行</span>
          </button>
        </div>
      </div>
    </header>
  );
}
