export interface WatchlistItem {
  ticker: string;
  company_name: string;
  target_discount_rate: number;
  current_price: number;
  base_price: number;
  bull_price: number;
  bear_price: number;
  divergence_rate: number;
  is_undervalued_alert: boolean;
  wacc: number;
  growth_rate_base: number;
  growth_rate_bull?: number;
  growth_rate_bear?: number;
  terminal_growth_rate?: number;
  implied_growth_rate?: number | null;
  net_cash?: number;
  free_cash_flow?: number;
  shares_outstanding?: number;
  reasoning_ja: string;
  last_earnings_date: string | null;
  calculated_at: string;
}

export interface WatchlistMeta {
  ticker: string;
  company_name: string;
  target_discount_rate: number;
  last_earnings_date: string | null;
  created_at?: string;
  updated_at?: string;
}

export interface TheoreticalPriceRecord {
  id: number;
  ticker?: string;
  trigger_type: "INITIAL" | "EARNINGS_UPDATE" | "MANUAL" | string;
  base_price: number;
  bull_price: number;
  bear_price: number;
  net_cash: number;
  free_cash_flow: number;
  shares_outstanding: number;
  wacc: number;
  growth_rate_base: number;
  growth_rate_bull: number;
  growth_rate_bear: number;
  terminal_growth_rate: number;
  reasoning_ja: string;
  calculated_at: string;
}

export interface DailyPriceRecord {
  id: number;
  ticker: string;
  trade_date: string;
  close_price: number;
  divergence_rate: number;
  created_at: string;
}

export interface AlertLog {
  id: number;
  ticker: string;
  alert_type: "UNDERVALUED" | "EARNINGS" | "NEWS_SUMMARY" | "INITIAL_CALC" | string;
  message: string;
  sent_at: string;
}

export interface StockDetail {
  watchlist: WatchlistMeta;
  current_price?: number;
  divergence_rate?: number;
  is_undervalued_alert?: boolean;
  implied_growth_rate?: number | null;
  auto_wacc?: number | null;
  auto_terminal_growth_rate?: number | null;
  beta?: number | null;
  risk_free_rate?: number | null;
  effective_tax_rate?: number | null;
  latest_revenue?: number | null;
  fcf_margin?: number | null;
  latest_theoretical: TheoreticalPriceRecord | null;
  theoretical_history: TheoreticalPriceRecord[];
  daily_prices?: DailyPriceRecord[];
  recent_alerts: AlertLog[];
}

export interface JobSummaryResponse {
  job_name: string;
  processed_count: number;
  alerts_triggered: number;
  messages: string[];
  executed_at: string;
}
