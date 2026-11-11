import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "kinun-signal JP | 日本株 理論株価アナリスト",
  description:
    "日本株DCF理論株価算出・日次割安アラート・四半期決算自動再計算ターミナル（データ: J-Quants / EDINET）",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="ja" className="dark" suppressHydrationWarning>
      <body className="min-h-screen bg-slate-50 text-slate-900 dark:bg-slate-950 dark:text-slate-100 font-sans transition-colors duration-200">
        {children}
      </body>
    </html>
  );
}
