"use client";

import React from "react";
import Link from "next/link";
import { Check, X } from "lucide-react";

export type Market = "us" | "jp";

interface SideMenuProps {
  open: boolean;
  onClose: () => void;
  current: Market;
}

const MARKET_LINKS: {
  market: Market;
  href: string;
  label: string;
  subLabel: string;
}[] = [
  {
    market: "us",
    href: "/",
    label: "米国株ダッシュボード",
    subLabel: "Personal Stock Analyst",
  },
  {
    market: "jp",
    href: "/jp",
    label: "日本株ダッシュボード",
    subLabel: "kinun-signal JP",
  },
];

/**
 * Slide-in side menu opened from the Header's hamburger button.
 * Its main job is switching between the US and Japanese dashboards.
 * Like the app's modals, it closes via its buttons / backdrop rather
 * than the Escape key.
 */
export default function SideMenu({ open, onClose, current }: SideMenuProps) {
  if (!open) {
    return null;
  }

  return (
    <div className="fixed inset-0 z-50">
      {/* Backdrop */}
      <button
        type="button"
        aria-label="メニューを閉じる"
        onClick={onClose}
        className="absolute inset-0 h-full w-full cursor-default bg-slate-950/60 backdrop-blur-[2px]"
      />

      {/* Panel */}
      <aside className="absolute inset-y-0 left-0 flex w-72 max-w-[85vw] flex-col border-r border-slate-200 bg-white shadow-2xl dark:border-slate-800 dark:bg-slate-900">
        <div className="flex items-center justify-between border-b border-slate-200 px-4 py-3.5 dark:border-slate-800">
          <span className="text-sm font-bold tracking-tight text-slate-900 dark:text-slate-100">
            メニュー
          </span>
          <button
            type="button"
            onClick={onClose}
            aria-label="メニューを閉じる"
            className="inline-flex h-8 w-8 items-center justify-center rounded-lg border border-slate-300 bg-white text-slate-600 transition hover:bg-slate-50 hover:text-slate-900 dark:border-slate-700 dark:bg-slate-900 dark:text-slate-300 dark:hover:bg-slate-800 dark:hover:text-white"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        <nav className="flex flex-col gap-1.5 p-3" aria-label="市場切替">
          {MARKET_LINKS.map((item) => {
            const isCurrent = item.market === current;
            return (
              <Link
                key={item.market}
                href={item.href}
                onClick={onClose}
                aria-current={isCurrent ? "page" : undefined}
                className={
                  isCurrent
                    ? "flex items-center justify-between rounded-lg border border-emerald-600/40 bg-emerald-50 px-3.5 py-3 dark:border-emerald-500/40 dark:bg-emerald-500/10"
                    : "flex items-center justify-between rounded-lg border border-transparent px-3.5 py-3 transition hover:bg-slate-100 dark:hover:bg-slate-800"
                }
              >
                <span>
                  <span className="block text-sm font-semibold text-slate-900 dark:text-slate-100">
                    {item.label}
                  </span>
                  <span className="block text-xs text-slate-500 dark:text-slate-400">
                    {item.subLabel}
                  </span>
                </span>
                {isCurrent && (
                  <Check className="h-4 w-4 shrink-0 text-emerald-600 dark:text-emerald-400" />
                )}
              </Link>
            );
          })}
        </nav>
      </aside>
    </div>
  );
}
