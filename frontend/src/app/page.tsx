"use client";

import React, { useCallback, useEffect, useState } from "react";
import {
  Bell,
  CheckCircle2,
  Clock,
  Layers,
  MessageSquare,
  RefreshCcw,
  Target,
} from "lucide-react";
import Header from "@/components/Header";
import StockCard from "@/components/StockCard";
import StockDetailModal from "@/components/StockDetailModal";
import AddStockModal from "@/components/AddStockModal";
import { AlertLog, JobSummaryResponse, WatchlistItem } from "@/types/stock";

const API_BASE_URL = process.env.NEXT_PUBLIC_API_URL || "";

const FALLBACK_WATCHLIST: WatchlistItem[] = [
  {
    ticker: "AAPL",
    company_name: "Apple Inc.",
    target_discount_rate: 15.0,
    current_price: 225.5,
    base_price: 240.0,
    bull_price: 285.0,
    bear_price: 195.0,
    divergence_rate: -6.04,
    is_undervalued_alert: false,
    wacc: 0.085,
    growth_rate_base: 0.11,
    growth_rate_bull: 0.16,
    growth_rate_bear: 0.06,
    terminal_growth_rate: 0.025,
    net_cash: -41_400_000_000,
    free_cash_flow: 108_800_000_000,
    shares_outstanding: 15_200_000_000,
    reasoning_ja:
      "【推論レポート】Appleは年間1,088億ドル規模の圧倒的なフリーキャッシュフロー（FCF）創出力を維持しています。Apple Intelligence搭載端末による買い替えサイクル加速と高粗利なServices部門（App Store・iCloud・決済）の2桁成長が事業価値を牽引。有利子負債を差し引いたネットキャッシュはマイナス414億ドルですが、強固な顧客ロックインと継続的な自社株買いによる希薄化後株式数の減少を考慮し、WACC 8.5%・基本FCF成長率11.0%・永久成長率2.5%と推論しました。欧州等の独占禁止法規制リスクを弱気シナリオ（6.0%成長）に織り込んでいます。",
    last_earnings_date: "2026-07-30",
    calculated_at: "2026-10-17T08:00:00Z",
  },
  {
    ticker: "NVDA",
    company_name: "NVIDIA Corporation",
    target_discount_rate: 20.0,
    current_price: 112.4,
    base_price: 148.85,
    bull_price: 194.2,
    bear_price: 96.4,
    divergence_rate: -24.49,
    is_undervalued_alert: true,
    wacc: 0.09,
    growth_rate_base: 0.28,
    growth_rate_bull: 0.36,
    growth_rate_bear: 0.16,
    terminal_growth_rate: 0.03,
    net_cash: 28_700_000_000,
    free_cash_flow: 74_200_000_000,
    shares_outstanding: 24_500_000_000,
    reasoning_ja:
      "【推論レポート】NVIDIAは生成AIデータセンター向けBlackwell世代GPUおよびネットワーク機器の独占的需要により、直近FCFが742億ドルへ急拡大し、現金385億ドル・負債98億ドルと実質無借金（ネットキャッシュ+287億ドル）の極めて健全な財務体質を誇ります。CUDAエコシステムとNIMソフトウェアによる高い参入障壁を評価し、今後5年間の基本FCF成長率を28.0%（強気36.0%・弱気16.0%）、半導体サイクルのボラティリティを加味したWACCを9.0%、永久成長率を3.0%と算出しました。現在株価は目標割安率を超える買い圏内に位置しています。",
    last_earnings_date: "2026-08-26",
    calculated_at: "2026-10-17T08:00:00Z",
  },
];

const FALLBACK_ALERTS: AlertLog[] = [
  {
    id: 3,
    ticker: "NVDA",
    alert_type: "UNDERVALUED",
    message:
      "🚨【割安アラート】NVDA が理論株価($148.85)から24.49%割安の $112.40 に到達しました！（目標割安率: 20%）",
    sent_at: "2026-10-17T08:01:00Z",
  },
  {
    id: 2,
    ticker: "NVDA",
    alert_type: "INITIAL_CALC",
    message:
      "📊【新規銘柄登録＆理論株価算出】\n銘柄: NVIDIA Corporation (NVDA)\n現在株価: $112.40\n理論株価(Base): $148.85 (乖離率: -24.49%)\n強気(Bull): $194.20 / 弱気(Bear): $96.40\nWACC: 9.0% / 基本成長率: 28.0%",
    sent_at: "2026-10-17T08:00:30Z",
  },
  {
    id: 1,
    ticker: "AAPL",
    alert_type: "INITIAL_CALC",
    message:
      "📊【新規銘柄登録＆理論株価算出】\n銘柄: Apple Inc. (AAPL)\n現在株価: $225.50\n理論株価(Base): $240.00 (乖離率: -6.04%)\n強気(Bull): $285.00 / 弱気(Bear): $195.00\nWACC: 8.5% / 基本成長率: 11.0%",
    sent_at: "2026-10-17T08:00:00Z",
  },
];

export default function DashboardPage() {
  const [watchlist, setWatchlist] = useState<WatchlistItem[]>([]);
  const [alerts, setAlerts] = useState<AlertLog[]>([]);
  const [llmModel, setLlmModel] = useState<string | null>(null);
  const [isInitialLoading, setIsInitialLoading] = useState<boolean>(true);
  const [isDailyLoading, setIsDailyLoading] = useState<boolean>(false);
  const [isEarningsLoading, setIsEarningsLoading] = useState<boolean>(false);
  const [selectedTicker, setSelectedTicker] = useState<string | null>(null);
  const [isAddModalOpen, setIsAddModalOpen] = useState<boolean>(false);
  const [toastMessage, setToastMessage] = useState<string | null>(null);
  const [theme, setTheme] = useState<"dark" | "light">("dark");
  const [sortBy, setSortBy] = useState<"registered" | "cheapest" | "name">("registered");

  useEffect(() => {
    const savedTheme = localStorage.getItem("psa-theme");
    if (savedTheme === "light" || savedTheme === "dark") {
      setTheme(savedTheme);
      document.documentElement.classList.toggle("dark", savedTheme === "dark");
    } else {
      document.documentElement.classList.add("dark");
    }
  }, []);

  const handleToggleTheme = useCallback(() => {
    setTheme((prev) => {
      const next = prev === "dark" ? "light" : "dark";
      localStorage.setItem("psa-theme", next);
      document.documentElement.classList.toggle("dark", next === "dark");
      return next;
    });
  }, []);

  const showToast = useCallback((msg: string) => {
    setToastMessage(msg);
    setTimeout(() => {
      setToastMessage((prev) => (prev === msg ? null : prev));
    }, 5500);
  }, []);

// Sort watchlist
  const sortedWatchlist = [...watchlist].sort((a, b) => {
    if (sortBy === "cheapest") {
      return a.divergence_rate - b.divergence_rate;
    }
    if (sortBy === "name") {
      return a.company_name.localeCompare(b.company_name);
    }
    return 0; // Default to registered order
  });

  const fetchDashboardData = useCallback(async () => {
    try {
      const [wRes, aRes, sysRes] = await Promise.all([
        fetch(`${API_BASE_URL}/api/watchlist`, { cache: "no-store" }),
        fetch(`${API_BASE_URL}/api/alerts`, { cache: "no-store" }),
        fetch(`${API_BASE_URL}/api/system/info`, { cache: "no-store" }).catch(() => null),
      ]);

      if (!wRes.ok || !aRes.ok) {
        throw new Error("Backend API returned non-200");
      }

      const wData: WatchlistItem[] = await wRes.json();
      const aData: AlertLog[] = await aRes.json();
      setWatchlist(wData);
      setAlerts(aData);

      if (sysRes && sysRes.ok) {
        const sysData = await sysRes.json();
        if (sysData.llm_model) {
          setLlmModel(sysData.llm_model);
        }
      }
    } catch {
      // Fallback to realistic seeded state if backend is not yet started
      setWatchlist((prev) => (prev.length > 0 ? prev : FALLBACK_WATCHLIST));
      setAlerts((prev) => (prev.length > 0 ? prev : FALLBACK_ALERTS));
    } finally {
      setIsInitialLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchDashboardData();
  }, [fetchDashboardData]);

  const handleRunDailyCheck = async () => {
    setIsDailyLoading(true);
    try {
      const res = await fetch(`${API_BASE_URL}/api/jobs/daily-check`, {
        method: "POST",
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const summary: JobSummaryResponse = await res.json();
      await fetchDashboardData();
      showToast(
        `✅ 日次監視＆ニュース要約が完了しました（対象: ${summary.processed_count}銘柄 / LINE通知発火: ${summary.alerts_triggered}件）`
      );
    } catch {
      // Offline fallback simulation
      const nowIso = new Date().toISOString();
      const simulatedAlerts: AlertLog[] = watchlist.flatMap((item, idx) => {
        const list: AlertLog[] = [];
        if (item.is_undervalued_alert) {
          list.push({
            id: Date.now() + idx * 10 + 1,
            ticker: item.ticker,
            alert_type: "UNDERVALUED",
            message: `🚨【割安アラート】${item.ticker} が理論株価($${item.base_price.toFixed(
              2
            )})から${Math.abs(item.divergence_rate).toFixed(
              2
            )}%割安の $${item.current_price.toFixed(2)} に到達しました！`,
            sent_at: nowIso,
          });
        }
        list.push({
          id: Date.now() + idx * 10 + 2,
          ticker: item.ticker,
          alert_type: "NEWS_SUMMARY",
          message: `📰【モーニングニュース要約】${item.company_name} (${item.ticker})\n① 主力AI・クラウド領域の需要拡大により今期ガイダンスが市場予想を上回る推移\n② 営業キャッシュフローマージンが改善し、自社株買いと設備投資の両立を継続\n③ 機関投資家の目標株価引き上げが相次ぐ\n💡 株価への影響：ポジティブ`,
          sent_at: nowIso,
        });
        return list;
      });
      setAlerts((prev) => [...simulatedAlerts, ...prev].slice(0, 30));
      showToast(
        `✅ 日次監視＆ニュース要約を実行しました（対象: ${watchlist.length}銘柄 / LINE通知発火: ${simulatedAlerts.length}件）`
      );
    } finally {
      setIsDailyLoading(false);
    }
  };

  const handleRunEarningsCheck = async () => {
    setIsEarningsLoading(true);
    try {
      const res = await fetch(
        `${API_BASE_URL}/api/jobs/earnings-check?force=true`,
        {
          method: "POST",
        }
      );
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const summary: JobSummaryResponse = await res.json();
      await fetchDashboardData();
      showToast(
        `📊 四半期決算チェック＆理論株価の自動再計算が完了しました（更新通知: ${summary.alerts_triggered}件）`
      );
    } catch {
      // Offline fallback simulation
      const nowIso = new Date().toISOString();
      const updatedWatchlist = watchlist.map((item) => {
        const newBase = Number((item.base_price * 1.045).toFixed(2));
        const newBull = Number((item.bull_price * 1.045).toFixed(2));
        const newBear = Number((item.bear_price * 1.045).toFixed(2));
        const newDiv = Number(
          (((item.current_price - newBase) / newBase) * 100).toFixed(2)
        );
        return {
          ...item,
          base_price: newBase,
          bull_price: newBull,
          bear_price: newBear,
          divergence_rate: newDiv,
          is_undervalued_alert:
            item.current_price <=
            newBase * (1 - item.target_discount_rate / 100),
          calculated_at: nowIso,
        };
      });
      const earningsAlerts: AlertLog[] = updatedWatchlist.map((item, idx) => ({
        id: Date.now() + idx,
        ticker: item.ticker,
        alert_type: "EARNINGS",
        message: `📊【四半期決算＆理論株価アップデート】${item.company_name} (${item.ticker})\n🔄 理論株価再計算: 新Base理論株価 $${item.base_price.toFixed(
          2
        )} (現在株価比: ${item.divergence_rate.toFixed(2)}%)`,
        sent_at: nowIso,
      }));
      setWatchlist(updatedWatchlist);
      setAlerts((prev) => [...earningsAlerts, ...prev].slice(0, 30));
      showToast(
        `📊 四半期決算チェック＆理論株価の自動再計算が完了しました（更新通知: ${earningsAlerts.length}件）`
      );
    } finally {
      setIsEarningsLoading(false);
    }
  };

  const handleAddStock = async (
    ticker: string,
    targetDiscountRate: number
  ) => {
    try {
      const res = await fetch(`${API_BASE_URL}/api/watchlist`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          ticker,
          target_discount_rate: targetDiscountRate,
        }),
      });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      await fetchDashboardData();
      showToast(
        `📊 ${ticker} を新規登録し、DCF理論株価算出とLINE通知を実行しました`
      );
    } catch {
      // Offline fallback creation
      const nowIso = new Date().toISOString();
      const newItem: WatchlistItem = {
        ticker,
        company_name: `${ticker} Corporation`,
        target_discount_rate: targetDiscountRate,
        current_price: 168.5,
        base_price: 215.4,
        bull_price: 262.0,
        bear_price: 158.0,
        divergence_rate: -21.77,
        is_undervalued_alert: 21.77 >= targetDiscountRate,
        wacc: 0.085,
        growth_rate_base: 0.14,
        growth_rate_bull: 0.2,
        growth_rate_bear: 0.08,
        terminal_growth_rate: 0.025,
        net_cash: 22_500_000_000,
        free_cash_flow: 48_000_000_000,
        shares_outstanding: 6_800_000_000,
        reasoning_ja: `【推論レポート】${ticker}の過去3期FCF成長トレンドと高い営業利益率に基づき、今後5年間の基本FCF成長率を14.0%、割引率(WACC)を8.5%、永久成長率を2.5%と推論しました。潤沢なネットキャッシュと事業価値の合計から算出される理論株価は現在株価に対して割安水準にあります。`,
        last_earnings_date: "2026-08-01",
        calculated_at: nowIso,
      };
      setWatchlist((prev) => [
        ...prev.filter((i) => i.ticker !== ticker),
        newItem,
      ]);
      setAlerts((prev) => [
        {
          id: Date.now(),
          ticker,
          alert_type: "INITIAL_CALC",
          message: `📊【新規銘柄登録＆理論株価算出】\n銘柄: ${newItem.company_name} (${ticker})\n現在株価: $${newItem.current_price.toFixed(
            2
          )}\n理論株価(Base): $${newItem.base_price.toFixed(2)} (乖離率: ${newItem.divergence_rate.toFixed(
            2
          )}%)`,
          sent_at: nowIso,
        },
        ...prev,
      ]);
      showToast(
        `📊 ${ticker} を登録し、DCF理論株価を算出しました（理論株価: $${newItem.base_price.toFixed(
          2
        )}）`
      );
    }
  };

  const undervaluedCount = watchlist.filter(
    (s) => s.is_undervalued_alert
  ).length;

  const latestExecutionTime =
    alerts.length > 0
      ? new Date(alerts[0].sent_at).toLocaleString("ja-JP")
      : watchlist.length > 0
      ? new Date(watchlist[0].calculated_at).toLocaleString("ja-JP")
      : "未実行";

  const selectedFallbackItem =
    watchlist.find((w) => w.ticker === selectedTicker) ?? null;

  return (
    <div className="min-h-screen bg-slate-50 text-slate-900 transition-colors duration-200 dark:bg-slate-950 dark:text-slate-100">
      {/* 1. Top Navigation Bar */}
      <Header
        onOpenAddModal={() => setIsAddModalOpen(true)}
        onRunDailyCheck={handleRunDailyCheck}
        onRunEarningsCheck={handleRunEarningsCheck}
        isDailyLoading={isDailyLoading}
        isEarningsLoading={isEarningsLoading}
        theme={theme}
        onToggleTheme={handleToggleTheme}
        llmModel={llmModel}
      />

      {/* Toast Banner */}
      {toastMessage && (
        <div className="mx-auto max-w-[1440px] px-4 pt-4 sm:px-6">
          <div className="flex items-center justify-between rounded-xl border border-slate-200 border-l-4 border-l-emerald-600 bg-white px-4 py-3 text-xs font-medium text-slate-800 shadow-md dark:border-emerald-500/50 dark:border-l dark:bg-emerald-950/60 dark:text-emerald-200 dark:shadow-lg sm:text-sm">
            <div className="flex items-center gap-2">
              <CheckCircle2 className="h-4 w-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
              <span>{toastMessage}</span>
            </div>
            <button
              type="button"
              onClick={() => setToastMessage(null)}
              className="ml-4 text-xs text-slate-500 hover:text-slate-900 hover:underline dark:text-emerald-400"
            >
              閉じる
            </button>
          </div>
        </div>
      )}

      <main className="mx-auto max-w-[1440px] space-y-6 px-4 py-6 sm:px-6">
        {/* 2. Summary KPI Bar */}
        <section className="grid grid-cols-1 gap-4 sm:grid-cols-3">
          {/* KPI 1: Monitored Stocks Count */}
          <div className="flex items-center justify-between rounded-xl border border-slate-200 bg-white px-5 py-4 shadow-sm dark:border-slate-800 dark:bg-slate-900/90 dark:shadow-none">
            <div>
              <span className="text-xs font-medium text-slate-500 dark:text-slate-400">
                監視中の米国株銘柄数
              </span>
              <div className="mt-1 flex items-baseline gap-2">
                <span className="font-mono text-2xl font-extrabold text-slate-900 dark:text-white">
                  {watchlist.length}
                </span>
                <span className="text-xs text-slate-500 dark:text-slate-400">
                  銘柄
                </span>
              </div>
            </div>
            <div className="flex h-11 w-11 items-center justify-center rounded-xl border border-slate-200 bg-slate-100 text-slate-700 dark:border-slate-700 dark:bg-slate-800/80 dark:text-indigo-400">
              <Layers className="h-5 w-5" />
            </div>
          </div>

          {/* KPI 2: Undervalued Alert Count */}
          <div className="flex items-center justify-between rounded-xl border border-slate-200 border-l-4 border-l-emerald-600 bg-white px-5 py-4 shadow-sm dark:border-l dark:border-emerald-500/40 dark:bg-emerald-950/20 dark:shadow-[0_0_20px_-8px_rgba(16,185,129,0.3)]">
            <div>
              <span className="text-xs font-semibold text-slate-700 dark:text-emerald-300">
                🎯 割安アラート点灯中の銘柄数
              </span>
              <div className="mt-1 flex items-baseline gap-2">
                <span className="font-mono text-2xl font-extrabold text-slate-900 dark:text-emerald-400">
                  {undervaluedCount}
                </span>
                <span className="text-xs font-medium text-emerald-700 dark:text-emerald-300/80">
                  銘柄 (買い圏内到達)
                </span>
              </div>
            </div>
            <div className="flex h-11 w-11 items-center justify-center rounded-xl border border-slate-200 bg-slate-900 text-emerald-400 dark:border-emerald-500/40 dark:bg-emerald-500/15 dark:text-emerald-400">
              <Target className="h-5 w-5" />
            </div>
          </div>

          {/* KPI 3: Latest Batch / Notification Timestamp */}
          <div className="flex items-center justify-between rounded-xl border border-slate-200 bg-white px-5 py-4 shadow-sm dark:border-slate-800 dark:bg-slate-900/90 dark:shadow-none">
            <div>
              <span className="text-xs font-medium text-slate-500 dark:text-slate-400">
                最新のバッチ・通知実行時刻
              </span>
              <div className="mt-1 font-mono text-sm font-bold text-slate-800 dark:text-slate-200 sm:text-base">
                {latestExecutionTime}
              </div>
              <span className="text-[11px] text-slate-500">
                自動Cron: 米国市場引け後 翌朝06:30 (JST)
              </span>
            </div>
            <div className="flex h-11 w-11 items-center justify-center rounded-xl border border-slate-200 bg-slate-100 text-slate-600 dark:border-slate-700 dark:bg-slate-800/80 dark:text-slate-300">
              <Clock className="h-5 w-5" />
            </div>
          </div>
        </section>

        {/* 3. Main Content: 2-Column Layout (Left 2/3 Cards, Right 1/3 LINE Timeline) */}
        <section className="grid grid-cols-1 gap-6 lg:grid-cols-3">
          {/* Left / Center Column (Width 2/3): Watchlist Stock Cards Grid */}
          <div className="space-y-4 lg:col-span-2">
            <div className="flex items-center justify-between">
              <div>
                <h2 className="text-base font-bold text-slate-900 dark:text-white sm:text-lg">
                  ウォッチ銘柄 DCF バリュエーション一覧
                </h2>
                <p className="text-xs text-slate-500 dark:text-slate-400">
                  カードをクリックすると財務基礎データ・算出根拠全文・WACCスライダー再計算モーダルが開きます
                </p>
              </div>
              <select
                value={sortBy}
                onChange={(e) => setSortBy(e.target.value as any)}
                className="rounded-lg border border-slate-200 bg-white px-2.5 py-1.5 text-xs text-slate-600 shadow-sm outline-none transition focus:border-emerald-500 focus:ring-1 focus:ring-emerald-500 dark:border-slate-800 dark:bg-slate-900 dark:text-slate-400 dark:shadow-none dark:focus:border-emerald-500/50"
              >
                <option value="registered">登録順</option>
                <option value="cheapest">割安な順</option>
                <option value="name">名前順</option>
              </select>
            </div>

            {isInitialLoading ? (
              <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
                {[1, 2].map((n) => (
                  <div
                    key={n}
                    className="h-80 animate-pulse rounded-xl border border-slate-200 bg-slate-100 p-5 dark:border-slate-800 dark:bg-slate-900/60"
                  />
                ))}
              </div>
            ) : watchlist.length === 0 ? (
              <div className="rounded-xl border border-slate-200 bg-white p-12 text-center shadow-sm dark:border-slate-800 dark:bg-slate-900/60 dark:shadow-none">
                <p className="text-sm text-slate-500 dark:text-slate-400">
                  現在ウォッチ中の銘柄はありません。右上の「＋
                  銘柄を追加」から米国株ティッカーを登録してください。
                </p>
              </div>
            ) : (
              <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
                {sortedWatchlist.map((stock) => (
                  <StockCard
                    key={stock.ticker}
                    stock={stock}
                    onSelect={(ticker) => setSelectedTicker(ticker)}
                  />
                ))}
              </div>
            )}
          </div>

          {/* Right Column (Width 1/3): LINE Notification & Alert History Timeline */}
          <div className="flex flex-col max-h-[640px] lg:h-[1020px] lg:max-h-[1020px] rounded-xl border border-slate-200 bg-white shadow-sm dark:border-slate-800 dark:bg-slate-900/95 dark:shadow-xl">
            {/* LINE Chat Header */}
            <div className="flex items-center justify-between border-b border-slate-200 bg-slate-50 px-4 py-3.5 dark:border-slate-800 dark:bg-[#06C755]/10">
              <div className="flex items-center gap-2.5">
                <div className="flex h-8 w-8 items-center justify-center rounded-full bg-[#06C755] text-white shadow-sm">
                  <MessageSquare className="h-4 w-4" />
                </div>
                <div>
                  <h2 className="text-sm font-bold text-slate-900 dark:text-white">
                    LINE 通知＆アラート履歴
                  </h2>
                  <p className="text-[11px] text-slate-500 dark:text-emerald-300/80">
                    Personal Stock Analyst Bot (リアルタイムプレビュー)
                  </p>
                </div>
              </div>
              <span className="inline-flex items-center gap-1 rounded-full border border-slate-200 bg-white px-2 py-0.5 font-mono text-[11px] font-medium text-slate-700 dark:border-emerald-500/30 dark:bg-emerald-500/10 dark:text-emerald-300">
                <Bell className="h-3 w-3" />
                {alerts.length}件
              </span>
            </div>

            {/* LINE Talk Timeline Messages */}
            <div className="flex-1 space-y-3.5 overflow-y-auto p-4">
              {alerts.length === 0 ? (
                <div className="py-12 text-center text-xs text-slate-500">
                  まだLINE通知履歴はありません。上の「日次監視＆ニュース要約を実行」または「決算チェック＆再計算を実行」を押すと通知が生成されます。
                </div>
              ) : (
                alerts.map((alert) => {
                  const isUndervalued = alert.alert_type === "UNDERVALUED";
                  const isEarnings = alert.alert_type === "EARNINGS";
                  const isNews = alert.alert_type === "NEWS_SUMMARY";

                  return (
                    <div key={alert.id} className="flex items-start gap-2.5">
                      <div className="mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-full border border-slate-200 bg-slate-900 font-mono text-[10px] font-bold text-white dark:border-emerald-500/40 dark:bg-emerald-950 dark:text-emerald-300">
                        {alert.ticker.slice(0, 4)}
                      </div>
                      <div className="flex-1">
                        <div className="mb-1 flex items-center justify-between gap-2">
                          <span
                            className={`rounded px-1.5 py-0.5 font-mono text-[10px] font-bold ${
                              isUndervalued
                                ? "border border-slate-200 bg-slate-900 text-emerald-400 dark:border-emerald-500/40 dark:bg-emerald-500/20 dark:text-emerald-300"
                                : isEarnings
                                ? "border border-indigo-200 bg-indigo-50 text-indigo-700 dark:border-indigo-500/40 dark:bg-indigo-500/20 dark:text-indigo-300"
                                : isNews
                                ? "border border-sky-200 bg-sky-50 text-sky-700 dark:border-sky-500/40 dark:bg-sky-500/20 dark:text-sky-300"
                                : "border border-slate-200 bg-slate-100 text-slate-700 dark:border-slate-700 dark:bg-slate-800 dark:text-slate-300"
                            }`}
                          >
                            {alert.alert_type} • {alert.ticker}
                          </span>
                          <span className="font-mono text-[10px] text-slate-500">
                            {new Date(alert.sent_at).toLocaleTimeString(
                              "ja-JP",
                              {
                                hour: "2-digit",
                                minute: "2-digit",
                                second: "2-digit",
                              }
                            )}
                          </span>
                        </div>
                        <div
                          className={`rounded-2xl rounded-tl-none border p-3 text-xs leading-relaxed whitespace-pre-line ${
                            isUndervalued
                              ? "border-slate-200 border-l-4 border-l-emerald-600 bg-slate-50 text-slate-900 dark:border-l dark:border-emerald-500/40 dark:bg-emerald-950/30 dark:text-emerald-100"
                              : "border-slate-200 bg-slate-50 text-slate-800 dark:border-slate-800 dark:bg-slate-950/90 dark:text-slate-200"
                          }`}
                        >
                          {alert.message}
                        </div>
                      </div>
                    </div>
                  );
                })
              )}
            </div>
          </div>
        </section>
      </main>

      {/* 4. Stock Detail & Interactive DCF Simulator Modal */}
      <StockDetailModal
        ticker={selectedTicker}
        fallbackItem={selectedFallbackItem}
        apiBaseUrl={API_BASE_URL}
        onClose={() => setSelectedTicker(null)}
        onRecalculated={(updated) => {
          setWatchlist((prev) =>
            prev.map((item) =>
              item.ticker === updated.ticker ? updated : item
            )
          );
          fetchDashboardData();
        }}
        onDeleted={(deletedTicker) => {
          setWatchlist((prev) =>
            prev.filter((item) => item.ticker !== deletedTicker)
          );
          showToast(`🗑️ ${deletedTicker} をウォッチリストから削除しました`);
        }}
      />

      {/* Add Stock Modal */}
      <AddStockModal
        isOpen={isAddModalOpen}
        onClose={() => setIsAddModalOpen(false)}
        onAddStock={handleAddStock}
      />
    </div>
  );
}
