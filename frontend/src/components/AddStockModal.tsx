"use client";

import React, { useState } from "react";
import { Cpu, Loader2, PlusCircle, Sparkles, X } from "lucide-react";

interface AddStockModalProps {
  isOpen: boolean;
  onClose: () => void;
  onAddStock: (ticker: string, targetDiscountRate: number) => Promise<void>;
}

const PRESET_TICKERS = [
  { ticker: "MSFT", name: "Microsoft" },
  { ticker: "GOOGL", name: "Alphabet" },
  { ticker: "AMZN", name: "Amazon" },
  { ticker: "META", name: "Meta Platforms" },
  { ticker: "TSLA", name: "Tesla" },
];

export default function AddStockModal({
  isOpen,
  onClose,
  onAddStock,
}: AddStockModalProps) {
  const [ticker, setTicker] = useState("");
  const [targetDiscountRate, setTargetDiscountRate] = useState<number>(20);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  if (!isOpen) return null;

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const cleaned = ticker.trim().toUpperCase();
    if (!cleaned) {
      setErrorMsg("ティッカーシンボルを入力してください（例: MSFT, GOOGL）");
      return;
    }

    setIsSubmitting(true);
    setErrorMsg(null);
    try {
      await onAddStock(cleaned, targetDiscountRate);
      setTicker("");
      setTargetDiscountRate(20);
      onClose();
    } catch (err) {
      setErrorMsg(
        err instanceof Error
          ? err.message
          : "銘柄の登録中にエラーが発生しました。"
      );
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/75 p-4 backdrop-blur-sm">
      <div className="w-full max-w-md overflow-hidden rounded-2xl border border-slate-800 bg-slate-900 shadow-2xl">
        {/* Header */}
        <div className="flex items-center justify-between border-b border-slate-800 bg-slate-950/80 px-6 py-4">
          <div className="flex items-center gap-2">
            <PlusCircle className="h-5 w-5 text-emerald-400" />
            <h2 className="text-base font-bold text-white">
              米国株ウォッチ銘柄を新規登録
            </h2>
          </div>
          <button
            type="button"
            onClick={onClose}
            disabled={isSubmitting}
            className="rounded-lg border border-slate-800 bg-slate-900 p-1.5 text-slate-400 transition hover:text-white"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* Form */}
        <form onSubmit={handleSubmit} className="space-y-5 p-6">
          <div>
            <label
              htmlFor="ticker-input"
              className="block text-xs font-semibold uppercase tracking-wider text-slate-300"
            >
              ティッカーシンボル (米国株)
            </label>
            <input
              id="ticker-input"
              type="text"
              placeholder="例: MSFT, GOOGL, AMZN"
              value={ticker}
              onChange={(e) => setTicker(e.target.value.toUpperCase())}
              disabled={isSubmitting}
              className="mt-1.5 w-full rounded-lg border border-slate-700 bg-slate-950 px-3.5 py-2.5 font-mono text-base font-bold uppercase text-white placeholder-slate-600 focus:border-emerald-500 focus:outline-none"
            />

            {/* Quick select chips */}
            <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
              <span className="text-[11px] text-slate-500">クイック選択:</span>
              {PRESET_TICKERS.map((p) => (
                <button
                  key={p.ticker}
                  type="button"
                  disabled={isSubmitting}
                  onClick={() => setTicker(p.ticker)}
                  className="rounded border border-slate-800 bg-slate-950 px-2 py-0.5 font-mono text-xs text-slate-300 transition hover:border-emerald-500/50 hover:text-emerald-300"
                >
                  {p.ticker}
                </button>
              ))}
            </div>
          </div>

          <div>
            <div className="flex items-center justify-between text-xs">
              <label
                htmlFor="discount-input"
                className="font-semibold uppercase tracking-wider text-slate-300"
              >
                目標割安アラート閾値 (%)
              </label>
              <span className="font-mono text-sm font-bold text-emerald-400">
                理論株価より {targetDiscountRate}% 以上割安で通知
              </span>
            </div>
            <input
              id="discount-input"
              type="range"
              min={5}
              max={40}
              step={5}
              value={targetDiscountRate}
              onChange={(e) => setTargetDiscountRate(Number(e.target.value))}
              disabled={isSubmitting}
              className="mt-2 h-2 w-full cursor-pointer appearance-none rounded-lg bg-slate-800 accent-emerald-500"
            />
            <div className="relative mt-2 h-4 font-mono text-[10px] text-slate-500">
              <span className="absolute left-[0%] top-0 -translate-x-0">5%</span>
              <span className="absolute left-[28.5%] top-0 -translate-x-1/2">15%</span>
              <span className="absolute left-[42.8%] top-0 -translate-x-1/2">20% (推奨)</span>
              <span className="absolute left-[71.4%] top-0 -translate-x-1/2">30%</span>
              <span className="absolute left-[100%] top-0 -translate-x-full">40%</span>
            </div>
          </div>

          <div className="rounded-xl border border-indigo-500/30 bg-indigo-950/20 p-3.5 text-xs text-indigo-200">
            <div className="mb-1 flex items-center gap-1.5 font-semibold text-indigo-300">
              <Cpu className="h-3.5 w-3.5" />
              <span>登録時の自動パイプライン処理</span>
            </div>
            <p className="leading-relaxed text-slate-300">
              FMP財務データ取得 ➔ LLM (<strong>argon</strong>) で成長率・WACC推論
              ➔ DCF理論株価算出 ➔ LINEへ初回分析レポート通知まで自動実行します。
            </p>
          </div>

          {errorMsg && (
            <div className="rounded-lg border border-rose-500/40 bg-rose-950/40 px-3 py-2 text-xs text-rose-300">
              {errorMsg}
            </div>
          )}

          <div className="flex items-center justify-end gap-2.5 pt-2">
            <button
              type="button"
              onClick={onClose}
              disabled={isSubmitting}
              className="rounded-lg border border-slate-700 bg-slate-800 px-4 py-2 text-xs font-semibold text-slate-300 hover:bg-slate-700"
            >
              キャンセル
            </button>
            <button
              type="submit"
              disabled={isSubmitting}
              className="inline-flex items-center gap-1.5 rounded-lg bg-emerald-600 px-4 py-2 text-xs font-bold text-white shadow transition hover:bg-emerald-500 disabled:opacity-50"
            >
              {isSubmitting ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  <span>argon 推論＆DCF算出中...</span>
                </>
              ) : (
                <>
                  <Sparkles className="h-4 w-4" />
                  <span>登録して理論株価を算出</span>
                </>
              )}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
