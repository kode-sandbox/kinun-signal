import type { Metadata } from "next";
import React from "react";

export const metadata: Metadata = {
  title: "kinun-signal JP | 日本株 理論株価アナリスト",
  description:
    "日本株DCF理論株価算出・日次割安アラート・四半期決算自動再計算ターミナル（データ: J-Quants / EDINET）",
};

export default function JpLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return <>{children}</>;
}
