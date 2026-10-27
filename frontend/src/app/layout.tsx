import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Personal Stock Analyst | Powered by argon LLM",
  description:
    "米国株DCF理論株価算出・日次割安アラート・英語ニュース3行要約・四半期決算自動再計算ターミナル",
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
