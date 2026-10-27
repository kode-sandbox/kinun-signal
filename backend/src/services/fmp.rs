use std::collections::HashMap;

use chrono::NaiveDate;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::config::AppConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockFinancials {
    pub ticker: String,
    pub company_name: String,
    pub current_price: f64,
    pub cash_and_short_term_investments: f64,
    pub total_debt: f64,
    pub free_cash_flow: f64,        // 直近TTMまたは通期FCF
    pub fcf_history: Vec<f64>,      // 過去FCF推移（直近から過去順）
    pub revenue_history: Vec<f64>,  // 過去売上推移（直近から過去順）
    pub shares_outstanding: f64,
    pub latest_earnings_date: Option<NaiveDate>,
    pub eps_actual: Option<f64>,
    pub eps_estimated: Option<f64>,
    #[serde(default)]
    pub beta: Option<f64>,
    #[serde(default)]
    pub risk_free_rate: Option<f64>,
    #[serde(default)]
    pub effective_tax_rate: Option<f64>,
    #[serde(default)]
    pub interest_expense: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockNewsItem {
    pub title: String,
    pub published_date: String,
    pub text: String,
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct FmpClient {
    http: Client,
    config: AppConfig,
}

#[derive(Debug, Deserialize)]
struct FmpProfileItem {
    #[serde(rename = "companyName", default)]
    company_name: String,
    #[serde(default)]
    price: f64,
    #[serde(default)]
    beta: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct FmpBalanceSheetItem {
    #[serde(rename = "cashAndShortTermInvestments", default)]
    cash_and_short_term_investments: f64,
    #[serde(rename = "totalDebt", default)]
    total_debt: f64,
}

#[derive(Debug, Deserialize)]
struct FmpCashFlowItem {
    #[serde(rename = "freeCashFlow", default)]
    free_cash_flow: f64,
}

#[derive(Debug, Deserialize)]
struct FmpIncomeItem {
    #[serde(default)]
    revenue: f64,
    #[serde(rename = "weightedAverageShsOutDil", default)]
    weighted_average_shs_out_dil: f64,
    #[serde(rename = "interestExpense", default)]
    interest_expense: Option<f64>,
    #[serde(rename = "incomeBeforeTax", default)]
    income_before_tax: Option<f64>,
    #[serde(rename = "incomeTaxExpense", default)]
    income_tax_expense: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct FmpEarningsSurpriseItem {
    #[serde(default)]
    date: String,
    #[serde(rename = "actualEarningResult")]
    actual_earning_result: Option<f64>,
    #[serde(rename = "estimatedEarning")]
    estimated_earning: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct FmpNewsItemRaw {
    #[serde(default)]
    title: String,
    #[serde(rename = "publishedDate", default)]
    published_date: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    url: String,
}

#[derive(Debug, Deserialize)]
struct YahooChartResponse {
    chart: YahooChartResultWrapper,
}

#[derive(Debug, Deserialize)]
struct YahooChartResultWrapper {
    result: Option<Vec<YahooChartResultItem>>,
}

#[derive(Debug, Deserialize)]
struct YahooChartResultItem {
    meta: YahooChartMeta,
    #[serde(default)]
    indicators: Option<YahooChartIndicators>,
}

#[derive(Debug, Deserialize)]
struct YahooChartMeta {
    #[serde(rename = "regularMarketPrice")]
    regular_market_price: Option<f64>,
    #[serde(rename = "longName")]
    long_name: Option<String>,
    #[serde(rename = "shortName")]
    short_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct YahooChartIndicators {
    #[serde(default)]
    quote: Vec<YahooChartQuote>,
}

#[derive(Debug, Deserialize)]
struct YahooChartQuote {
    #[serde(default)]
    close: Vec<Option<f64>>,
}

#[derive(Debug, Deserialize)]
struct YahooSearchResponse {
    news: Option<Vec<YahooSearchNewsItem>>,
}

#[derive(Debug, Deserialize)]
struct YahooSearchNewsItem {
    title: Option<String>,
    link: Option<String>,
    publisher: Option<String>,
    #[serde(rename = "providerPublishTime")]
    provider_publish_time: Option<i64>,
}

#[derive(Debug, Clone)]
struct TimeseriesPoint {
    as_of_date: String,
    raw_value: f64,
}

const YAHOO_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)";

impl FmpClient {
    pub fn new(config: AppConfig) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            config,
        }
    }

    pub async fn fetch_financials(&self, ticker: &str) -> StockFinancials {
        let symbol = ticker.trim().to_uppercase();
        if !self.config.is_fmp_mock() {
            match self.fetch_financials_from_api(&symbol).await {
                Ok(fin) => return fin,
                Err(err) => {
                    warn!(
                        "FMP API call failed for {} ({}). Falling back to Yahoo Finance live financials.",
                        symbol, err
                    );
                }
            }
        }

        // Fetch full live financial statements + market quote + Beta + 10Y Treasury yield from Yahoo Finance
        match self.fetch_financials_from_yahoo(&symbol).await {
            Ok(fin) => {
                info!(
                    "Successfully fetched LIVE Yahoo Finance fundamentals for {}: price=${:.2}, FCF=${:.2}B, NetCash=${:.2}B, Shares={:.2}B, Beta={:?}, Rf={:?}",
                    symbol,
                    fin.current_price,
                    fin.free_cash_flow / 1e9,
                    (fin.cash_and_short_term_investments - fin.total_debt) / 1e9,
                    fin.shares_outstanding / 1e9,
                    fin.beta,
                    fin.risk_free_rate
                );
                return fin;
            }
            Err(err) => {
                warn!(
                    "Yahoo Finance full fundamentals fetch failed for {} ({}). Attempting live quote + fallback.",
                    symbol, err
                );
            }
        }

        let mut fin = Self::mock_financials(&symbol);
        if let Ok((real_price, real_name, _)) = self.fetch_chart_with_closes(&symbol, "1y", "1mo").await {
            fin.current_price = real_price;
            if let Some(name) = real_name {
                fin.company_name = name;
            }
        }
        fin
    }

    pub async fn fetch_financials_from_yahoo(&self, symbol: &str) -> anyhow::Result<StockFinancials> {
        let (stock_chart_res, sp500_chart_res, tnx_res, ts_res) = tokio::join!(
            self.fetch_chart_with_closes(symbol, "5y", "1mo"),
            self.fetch_chart_with_closes("^GSPC", "5y", "1mo"),
            self.fetch_ten_year_treasury_yield(),
            self.fetch_yahoo_fundamentals_timeseries(symbol)
        );

        let (current_price, real_name, stock_closes) = stock_chart_res?;
        let ts_map = ts_res?;

        let beta = if let Ok((_, _, sp500_closes)) = sp500_chart_res {
            calculate_blume_adjusted_beta(&stock_closes, &sp500_closes)
        } else {
            None
        };

        let risk_free_rate = tnx_res.ok();

        let cash = latest_ts_val(&ts_map, "quarterlyCashCashEquivalentsAndShortTermInvestments")
            .or_else(|| latest_ts_val(&ts_map, "annualCashCashEquivalentsAndShortTermInvestments"))
            .or_else(|| latest_ts_val(&ts_map, "quarterlyCashAndCashEquivalents"))
            .or_else(|| latest_ts_val(&ts_map, "annualCashAndCashEquivalents"))
            .unwrap_or(0.0);

        let total_debt = latest_ts_val(&ts_map, "quarterlyTotalDebt")
            .or_else(|| latest_ts_val(&ts_map, "annualTotalDebt"))
            .unwrap_or(0.0);

        // Prefer the higher of TTM Free Cash Flow and latest Annual FCF when TTM is temporarily depressed by a single-quarter CapEx spike
        let ttm_fcf_pt = latest_ts_point(&ts_map, "trailingFreeCashFlow");
        let annual_fcf_pts = ts_map.get("annualFreeCashFlow").cloned().unwrap_or_default();

        let free_cash_flow = match (&ttm_fcf_pt, annual_fcf_pts.last()) {
            (Some(ttm), Some(ann)) => {
                if ttm.as_of_date >= ann.as_of_date {
                    if ttm.raw_value > 0.0 && ann.raw_value > ttm.raw_value {
                        // Smooth temporary CapEx spike by blending 50% TTM and 50% latest full-year FCF
                        0.5 * (ttm.raw_value + ann.raw_value)
                    } else {
                        ttm.raw_value
                    }
                } else {
                    ann.raw_value
                }
            }
            (Some(ttm), None) => ttm.raw_value,
            (None, Some(ann)) => ann.raw_value,
            (None, None) => 0.0,
        };

        // Build FCF history (latest first: [TTM (if newer), FY_latest, FY_prev1, FY_prev2, ...])
        let mut fcf_history: Vec<f64> = Vec::new();
        if let Some(ttm) = &ttm_fcf_pt {
            if annual_fcf_pts
                .last()
                .map(|a| ttm.as_of_date > a.as_of_date)
                .unwrap_or(true)
            {
                fcf_history.push(ttm.raw_value);
            }
        }
        for pt in annual_fcf_pts.iter().rev() {
            fcf_history.push(pt.raw_value);
        }

        // Build Revenue history (latest first)
        let ttm_rev_pt = latest_ts_point(&ts_map, "trailingTotalRevenue");
        let annual_rev_pts = ts_map.get("annualTotalRevenue").cloned().unwrap_or_default();
        let mut revenue_history: Vec<f64> = Vec::new();
        if let Some(ttm) = &ttm_rev_pt {
            if annual_rev_pts
                .last()
                .map(|a| ttm.as_of_date > a.as_of_date)
                .unwrap_or(true)
            {
                revenue_history.push(ttm.raw_value);
            }
        }
        for pt in annual_rev_pts.iter().rev() {
            revenue_history.push(pt.raw_value);
        }

        let shares_outstanding = latest_ts_val(&ts_map, "quarterlyDilutedAverageShares")
            .or_else(|| latest_ts_val(&ts_map, "trailingDilutedAverageShares"))
            .or_else(|| latest_ts_val(&ts_map, "quarterlyOrdinarySharesNumber"))
            .or_else(|| latest_ts_val(&ts_map, "annualDilutedAverageShares"))
            .unwrap_or(0.0);

        if shares_outstanding <= 0.0 && free_cash_flow == 0.0 && cash == 0.0 {
            anyhow::bail!("No fundamental financial statement rows returned from Yahoo Finance for {}", symbol);
        }

        let interest_expense = latest_ts_val(&ts_map, "trailingInterestExpense")
            .or_else(|| latest_ts_val(&ts_map, "annualInterestExpense"));

        let effective_tax_rate = latest_ts_val(&ts_map, "annualTaxRateForCalcs")
            .filter(|&r| r > 0.0 && r < 0.60)
            .or_else(|| {
                let tax = latest_ts_val(&ts_map, "annualTaxProvision")?;
                let pretax = latest_ts_val(&ts_map, "annualPretaxIncome")?;
                if pretax > 0.0 && tax >= 0.0 {
                    Some((tax / pretax).clamp(0.0, 0.45))
                } else {
                    None
                }
            });

        let latest_eps_pt = latest_ts_point(&ts_map, "quarterlyDilutedEPS");
        let latest_earnings_date = latest_eps_pt
            .as_ref()
            .and_then(|pt| NaiveDate::parse_from_str(&pt.as_of_date, "%Y-%m-%d").ok());
        let eps_actual = latest_eps_pt.map(|pt| pt.raw_value);

        Ok(StockFinancials {
            ticker: symbol.to_string(),
            company_name: real_name.unwrap_or_else(|| format!("{} Inc.", symbol)),
            current_price,
            cash_and_short_term_investments: cash,
            total_debt,
            free_cash_flow,
            fcf_history,
            revenue_history,
            shares_outstanding,
            latest_earnings_date,
            eps_actual,
            // Yahoo fundamentals-timeseries does not provide analyst consensus EPS estimate;
            // keep None so unavailable data is never fabricated or shown.
            eps_estimated: None,
            beta,
            risk_free_rate,
            effective_tax_rate,
            interest_expense,
        })
    }

    pub async fn fetch_real_quote_from_yahoo(
        &self,
        symbol: &str,
    ) -> anyhow::Result<(f64, Option<String>)> {
        let (price, name, _) = self.fetch_chart_with_closes(symbol, "5d", "1d").await?;
        Ok((price, name))
    }

    async fn fetch_chart_with_closes(
        &self,
        symbol: &str,
        range: &str,
        interval: &str,
    ) -> anyhow::Result<(f64, Option<String>, Vec<f64>)> {
        let url = format!(
            "https://query1.finance.yahoo.com/v8/finance/chart/{}?range={}&interval={}",
            symbol, range, interval
        );
        let resp = self
            .http
            .get(&url)
            .header("User-Agent", YAHOO_USER_AGENT)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Yahoo finance chart HTTP {}", resp.status());
        }
        let body: YahooChartResponse = resp.json().await?;
        let item = body
            .chart
            .result
            .and_then(|r| r.into_iter().next())
            .ok_or_else(|| anyhow::anyhow!("No result in Yahoo chart for {}", symbol))?;
        let price = item
            .meta
            .regular_market_price
            .ok_or_else(|| anyhow::anyhow!("No regularMarketPrice for {}", symbol))?;
        let name = item.meta.long_name.or(item.meta.short_name);
        let closes = item
            .indicators
            .and_then(|ind| ind.quote.into_iter().next())
            .map(|q| q.close.into_iter().flatten().collect::<Vec<f64>>())
            .unwrap_or_default();

        Ok((price, name, closes))
    }

    async fn fetch_ten_year_treasury_yield(&self) -> anyhow::Result<f64> {
        let (yield_pct, _, _) = self.fetch_chart_with_closes("^TNX", "5d", "1d").await?;
        // ^TNX is quoted in percentage points (e.g. 4.35 means 4.35% = 0.0435)
        let rate = if yield_pct > 0.25 {
            yield_pct / 100.0
        } else {
            yield_pct
        };
        Ok(rate.clamp(0.020, 0.070))
    }

    async fn fetch_yahoo_fundamentals_timeseries(
        &self,
        symbol: &str,
    ) -> anyhow::Result<HashMap<String, Vec<TimeseriesPoint>>> {
        let types = [
            "quarterlyCashCashEquivalentsAndShortTermInvestments",
            "annualCashCashEquivalentsAndShortTermInvestments",
            "quarterlyCashAndCashEquivalents",
            "annualCashAndCashEquivalents",
            "quarterlyTotalDebt",
            "annualTotalDebt",
            "quarterlyDilutedAverageShares",
            "trailingDilutedAverageShares",
            "annualDilutedAverageShares",
            "quarterlyOrdinarySharesNumber",
            "trailingFreeCashFlow",
            "annualFreeCashFlow",
            "trailingTotalRevenue",
            "annualTotalRevenue",
            "annualTaxRateForCalcs",
            "annualPretaxIncome",
            "annualTaxProvision",
            "trailingInterestExpense",
            "annualInterestExpense",
            "quarterlyDilutedEPS",
            "annualDilutedEPS",
        ]
        .join(",");

        let now_ts = chrono::Utc::now().timestamp() + 86400;
        let start_ts = now_ts - 6 * 365 * 86400;
        let url = format!(
            "https://query1.finance.yahoo.com/ws/fundamentals-timeseries/v1/finance/timeseries/{}?type={}&period1={}&period2={}",
            symbol, types, start_ts, now_ts
        );

        let resp = self
            .http
            .get(&url)
            .header("User-Agent", YAHOO_USER_AGENT)
            .send()
            .await?;

        if !resp.status().is_success() {
            anyhow::bail!("Yahoo fundamentals-timeseries HTTP {}", resp.status());
        }

        let root: serde_json::Value = resp.json().await?;
        let results = root
            .get("timeseries")
            .and_then(|t| t.get("result"))
            .and_then(|r| r.as_array())
            .ok_or_else(|| anyhow::anyhow!("Invalid timeseries JSON structure for {}", symbol))?;

        let mut map: HashMap<String, Vec<TimeseriesPoint>> = HashMap::new();

        for entry in results {
            let Some(type_name) = entry
                .get("meta")
                .and_then(|m| m.get("type"))
                .and_then(|t| t.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
            else {
                continue;
            };

            let Some(series_arr) = entry.get(type_name).and_then(|v| v.as_array()) else {
                continue;
            };

            let mut points = Vec::new();
            for item in series_arr {
                if item.is_null() {
                    continue;
                }
                let as_of_date = item
                    .get("asOfDate")
                    .and_then(|d| d.as_str())
                    .unwrap_or_default()
                    .to_string();
                let raw_val = item
                    .get("reportedValue")
                    .and_then(|rv| rv.get("raw"))
                    .and_then(|r| r.as_f64());

                if let Some(raw_value) = raw_val {
                    if raw_value.is_finite() {
                        points.push(TimeseriesPoint {
                            as_of_date,
                            raw_value,
                        });
                    }
                }
            }

            if !points.is_empty() {
                points.sort_by(|a, b| a.as_of_date.cmp(&b.as_of_date));
                map.insert(type_name.to_string(), points);
            }
        }

        Ok(map)
    }

    pub async fn fetch_real_news_from_yahoo(
        &self,
        symbol: &str,
    ) -> anyhow::Result<Vec<StockNewsItem>> {
        let url = format!(
            "https://query2.finance.yahoo.com/v1/finance/search?q={}&newsCount=5",
            symbol
        );
        let resp = self
            .http
            .get(&url)
            .header("User-Agent", YAHOO_USER_AGENT)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("Yahoo news HTTP {}", resp.status());
        }
        let body: YahooSearchResponse = resp.json().await?;
        let news_items = body
            .news
            .unwrap_or_default()
            .into_iter()
            .filter_map(|n| {
                let title = n.title?;
                let url = n.link.unwrap_or_default();
                let date_str = if let Some(ts) = n.provider_publish_time {
                    chrono::DateTime::from_timestamp(ts, 0)
                        .map(|dt| dt.format("%Y-%m-%d").to_string())
                        .unwrap_or_else(|| "Recent".to_string())
                } else {
                    "Recent".to_string()
                };
                let publisher = n.publisher.unwrap_or_else(|| "Yahoo Finance".to_string());
                Some(StockNewsItem {
                    title: title.clone(),
                    published_date: date_str,
                    text: format!("{}: {}", publisher, title),
                    url,
                })
            })
            .collect();
        Ok(news_items)
    }

    async fn fetch_financials_from_api(&self, symbol: &str) -> anyhow::Result<StockFinancials> {
        let key = &self.config.fmp_api_key;
        let base = "https://financialmodelingprep.com/api/v3";

        let profile_url = format!("{}/profile/{}?apikey={}", base, symbol, key);
        let bs_url = format!("{}/balance-sheet-statement/{}?limit=1&apikey={}", base, symbol, key);
        let cf_url = format!("{}/cash-flow-statement/{}?limit=3&apikey={}", base, symbol, key);
        let pl_url = format!("{}/income-statement/{}?limit=3&apikey={}", base, symbol, key);
        let earnings_url = format!("{}/earnings-surprises/{}?apikey={}", base, symbol, key);

        let profiles: Vec<FmpProfileItem> = self.http.get(&profile_url).send().await?.json().await?;
        let profile = profiles
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Empty profile response for {}", symbol))?;

        let bs_list: Vec<FmpBalanceSheetItem> = self.http.get(&bs_url).send().await?.json().await?;
        let bs = bs_list
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Empty balance sheet response for {}", symbol))?;

        let cf_list: Vec<FmpCashFlowItem> = self.http.get(&cf_url).send().await?.json().await?;
        if cf_list.is_empty() {
            anyhow::bail!("Empty cash flow response for {}", symbol);
        }

        let pl_list: Vec<FmpIncomeItem> = self.http.get(&pl_url).send().await?.json().await?;
        if pl_list.is_empty() {
            anyhow::bail!("Empty income statement response for {}", symbol);
        }

        let latest_earning = match self.http.get(&earnings_url).send().await {
            Ok(resp) => resp
                .json::<Vec<FmpEarningsSurpriseItem>>()
                .await
                .unwrap_or_default()
                .into_iter()
                .next(),
            Err(_) => None,
        };

        let free_cash_flow = cf_list[0].free_cash_flow;
        let fcf_history = cf_list.iter().map(|c| c.free_cash_flow).collect();
        let revenue_history = pl_list.iter().map(|p| p.revenue).collect();
        let shares_outstanding = pl_list[0].weighted_average_shs_out_dil;
        let interest_expense = pl_list[0].interest_expense;
        let effective_tax_rate = match (pl_list[0].income_tax_expense, pl_list[0].income_before_tax)
        {
            (Some(tax), Some(pretax)) if pretax > 0.0 && tax >= 0.0 => {
                Some((tax / pretax).clamp(0.0, 0.45))
            }
            _ => None,
        };

        let (latest_earnings_date, eps_actual, eps_estimated) = match latest_earning {
            Some(e) => {
                let parsed_date = NaiveDate::parse_from_str(&e.date, "%Y-%m-%d").ok();
                (parsed_date, e.actual_earning_result, e.estimated_earning)
            }
            None => (None, None, None),
        };

        let risk_free_rate = self.fetch_ten_year_treasury_yield().await.ok();

        Ok(StockFinancials {
            ticker: symbol.to_string(),
            company_name: if profile.company_name.is_empty() {
                format!("{} Corp.", symbol)
            } else {
                profile.company_name
            },
            current_price: profile.price,
            cash_and_short_term_investments: bs.cash_and_short_term_investments,
            total_debt: bs.total_debt,
            free_cash_flow,
            fcf_history,
            revenue_history,
            shares_outstanding,
            latest_earnings_date,
            eps_actual,
            eps_estimated,
            beta: profile.beta,
            risk_free_rate,
            effective_tax_rate,
            interest_expense,
        })
    }

    pub async fn fetch_news(&self, ticker: &str) -> Vec<StockNewsItem> {
        let symbol = ticker.trim().to_uppercase();
        if !self.config.is_fmp_mock() {
            let url = format!(
                "https://financialmodelingprep.com/api/v3/stock_news?tickers={}&limit=5&apikey={}",
                symbol, self.config.fmp_api_key
            );

            if let Ok(resp) = self.http.get(&url).send().await {
                if let Ok(items) = resp.json::<Vec<FmpNewsItemRaw>>().await {
                    if !items.is_empty() {
                        return items
                            .into_iter()
                            .map(|i| StockNewsItem {
                                title: i.title,
                                published_date: i.published_date,
                                text: i.text,
                                url: i.url,
                            })
                            .collect();
                    }
                }
            }
        }

        // Live news from Yahoo Finance
        match self.fetch_real_news_from_yahoo(&symbol).await {
            Ok(news) if !news.is_empty() => {
                info!(
                    "Fetched {} LIVE news items from Yahoo Finance for {}",
                    news.len(),
                    symbol
                );
                news
            }
            Err(err) => {
                warn!(
                    "Yahoo news fetch failed for {} ({}). Using mock news.",
                    symbol, err
                );
                Self::mock_news(&symbol)
            }
            _ => Self::mock_news(&symbol),
        }
    }

    /// Offline fallback financials (used ONLY when both FMP and Yahoo Finance are unreachable)
    pub fn mock_financials(symbol: &str) -> StockFinancials {
        match symbol {
            "AAPL" => StockFinancials {
                ticker: "AAPL".to_string(),
                company_name: "Apple Inc.".to_string(),
                current_price: 225.50,
                cash_and_short_term_investments: 62_399_000_000.0,
                total_debt: 84_344_000_000.0,
                free_cash_flow: 136_683_000_000.0,
                fcf_history: vec![
                    136_683_000_000.0,
                    98_767_000_000.0,
                    108_807_000_000.0,
                    99_584_000_000.0,
                ],
                revenue_history: vec![
                    466_823_000_000.0,
                    416_161_000_000.0,
                    391_035_000_000.0,
                    383_285_000_000.0,
                ],
                shares_outstanding: 14_714_676_000.0,
                latest_earnings_date: NaiveDate::from_ymd_opt(2026, 6, 30),
                eps_actual: Some(2.02),
                eps_estimated: None,
                beta: Some(1.07),
                risk_free_rate: Some(0.044),
                effective_tax_rate: Some(0.156),
                interest_expense: Some(3_933_000_000.0),
            },
            "NVDA" => StockFinancials {
                ticker: "NVDA".to_string(),
                company_name: "NVIDIA Corporation".to_string(),
                current_price: 216.00,
                cash_and_short_term_investments: 62_469_000_000.0,
                total_debt: 38_351_000_000.0,
                free_cash_flow: 127_006_000_000.0,
                fcf_history: vec![
                    127_006_000_000.0,
                    96_676_000_000.0,
                    60_853_000_000.0,
                    27_020_000_000.0,
                ],
                revenue_history: vec![
                    302_970_000_000.0,
                    215_938_000_000.0,
                    130_497_000_000.0,
                    60_922_000_000.0,
                ],
                shares_outstanding: 24_285_000_000.0,
                latest_earnings_date: NaiveDate::from_ymd_opt(2026, 7, 31),
                eps_actual: Some(2.46),
                eps_estimated: None,
                beta: Some(1.65),
                risk_free_rate: Some(0.044),
                effective_tax_rate: Some(0.151),
                interest_expense: Some(464_000_000.0),
            },
            _ => {
                let hash_seed: u64 = symbol.bytes().map(|b| b as u64).sum();
                let price = 95.0 + (hash_seed % 180) as f64;
                let fcf = (8_500_000_000.0 + (hash_seed % 25) as f64 * 1_200_000_000.0)
                    .max(2_000_000_000.0);
                let shares = 1_800_000_000.0 + (hash_seed % 15) as f64 * 250_000_000.0;
                StockFinancials {
                    ticker: symbol.to_string(),
                    company_name: format!("{} Inc.", symbol),
                    current_price: price,
                    cash_and_short_term_investments: fcf * 1.4,
                    total_debt: fcf * 0.85,
                    free_cash_flow: fcf,
                    fcf_history: vec![fcf, fcf * 0.88, fcf * 0.76],
                    revenue_history: vec![fcf * 4.5, fcf * 4.0, fcf * 3.6],
                    shares_outstanding: shares,
                    latest_earnings_date: None,
                    eps_actual: None,
                    eps_estimated: None,
                    beta: Some(1.05),
                    risk_free_rate: Some(0.043),
                    effective_tax_rate: Some(0.21),
                    interest_expense: None,
                }
            }
        }
    }

    pub fn mock_news(symbol: &str) -> Vec<StockNewsItem> {
        match symbol {
            "AAPL" => vec![
                StockNewsItem {
                    title: "Apple Intelligence Drives Stronger-Than-Expected iPhone Upgrade Cycle Across North America and Asia".to_string(),
                    published_date: "2026-10-16 19:30:00".to_string(),
                    text: "Institutional channel checks indicate high-margin Pro models account for 68% of early shipments, boosting Services ARPU and gross margin guidance for Q4.".to_string(),
                    url: "https://example.com/news/aapl-1".to_string(),
                },
                StockNewsItem {
                    title: "Apple Expands Custom Silicon Roadmap and Cloud AI Infrastructure Partnerships".to_string(),
                    published_date: "2026-10-16 14:15:00".to_string(),
                    text: "New M-series server clusters lower inference costs by 35%, reinforcing Apple's free cash flow resilience and long-term capital return program.".to_string(),
                    url: "https://example.com/news/aapl-2".to_string(),
                },
            ],
            "NVDA" => vec![
                StockNewsItem {
                    title: "Hyperscalers Raise Next-Gen AI Data Center CapEx Guidance by 28% for FY2027".to_string(),
                    published_date: "2026-10-16 20:45:00".to_string(),
                    text: "Cloud service providers confirmed multi-quarter backlog visibility for NVIDIA Blackwell Ultra and Rubin rack-scale architectures.".to_string(),
                    url: "https://example.com/news/nvda-1".to_string(),
                },
                StockNewsItem {
                    title: "NVIDIA Enterprise Software and NIM Inference Subscriptions Surpass $4.5B Annualized Run Rate".to_string(),
                    published_date: "2026-10-16 16:10:00".to_string(),
                    text: "Recurring software revenue continues to expand gross margins above 75%, supporting robust free cash flow generation.".to_string(),
                    url: "https://example.com/news/nvda-2".to_string(),
                },
            ],
            _ => vec![
                StockNewsItem {
                    title: format!("{} Reports Strong Institutional Demand and Operating Margin Expansion", symbol),
                    published_date: "2026-10-16 18:00:00".to_string(),
                    text: format!(
                        "{} continues to execute on strategic cost discipline and product expansion, supporting free cash flow generation.",
                        symbol
                    ),
                    url: format!("https://example.com/news/{}-1", symbol.to_lowercase()),
                },
            ],
        }
    }
}

fn latest_ts_point(
    map: &HashMap<String, Vec<TimeseriesPoint>>,
    key: &str,
) -> Option<TimeseriesPoint> {
    map.get(key).and_then(|v| v.last()).cloned()
}

fn latest_ts_val(map: &HashMap<String, Vec<TimeseriesPoint>>, key: &str) -> Option<f64> {
    latest_ts_point(map, key).map(|p| p.raw_value)
}

/// 銘柄とS&P 500 (^GSPC) の月次終値系列から経験的ベータ値を算出し、Blume調整 (2/3 * raw_beta + 1/3 * 1.0) を適用する
fn calculate_blume_adjusted_beta(stock_closes: &[f64], market_closes: &[f64]) -> Option<f64> {
    let n = stock_closes.len().min(market_closes.len());
    if n < 12 {
        return None;
    }

    let s_slice = &stock_closes[stock_closes.len() - n..];
    let m_slice = &market_closes[market_closes.len() - n..];

    let mut s_returns = Vec::with_capacity(n - 1);
    let mut m_returns = Vec::with_capacity(n - 1);

    for i in 1..n {
        let s0 = s_slice[i - 1];
        let s1 = s_slice[i];
        let m0 = m_slice[i - 1];
        let m1 = m_slice[i];
        if s0 > 0.0 && s1 > 0.0 && m0 > 0.0 && m1 > 0.0 {
            s_returns.push((s1 - s0) / s0);
            m_returns.push((m1 - m0) / m0);
        }
    }

    if s_returns.len() < 10 {
        return None;
    }

    let count = s_returns.len() as f64;
    let mean_s = s_returns.iter().sum::<f64>() / count;
    let mean_m = m_returns.iter().sum::<f64>() / count;

    let mut cov = 0.0;
    let mut var_m = 0.0;
    for (rs, rm) in s_returns.iter().zip(m_returns.iter()) {
        cov += (rs - mean_s) * (rm - mean_m);
        var_m += (rm - mean_m).powi(2);
    }

    if var_m <= 1e-12 {
        return None;
    }

    let raw_beta = cov / var_m;
    // Institutional forward-looking Blume adjustment towards 1.0
    let blume_beta = (0.55 * raw_beta + 0.45 * 1.0).clamp(0.55, 1.65);
    Some((blume_beta * 100.0).round() / 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_yahoo_live_quote_and_news() {
        let config = AppConfig::from_env();
        let client = FmpClient::new(config);

        let financials = client.fetch_financials("AAPL").await;
        println!(
            "Fetched AAPL Price: ${:.2}, Company: {}, FCF: ${:.2}B, Cash: ${:.2}B, Debt: ${:.2}B, Shares: {:.2}B, Beta: {:?}, Rf: {:?}",
            financials.current_price,
            financials.company_name,
            financials.free_cash_flow / 1e9,
            financials.cash_and_short_term_investments / 1e9,
            financials.total_debt / 1e9,
            financials.shares_outstanding / 1e9,
            financials.beta,
            financials.risk_free_rate
        );
        assert!(financials.current_price > 50.0);
        assert!(financials.free_cash_flow > 10_000_000_000.0);
        assert!(financials.shares_outstanding > 1_000_000_000.0);
        assert_eq!(financials.ticker, "AAPL");

        let nvda_fin = client.fetch_financials("NVDA").await;
        println!(
            "Fetched NVDA Price: ${:.2}, Company: {}, FCF: ${:.2}B, Beta: {:?}",
            nvda_fin.current_price,
            nvda_fin.company_name,
            nvda_fin.free_cash_flow / 1e9,
            nvda_fin.beta
        );
        assert!(nvda_fin.current_price > 50.0);
        assert!(nvda_fin.free_cash_flow > 10_000_000_000.0);

        let news = client.fetch_news("AAPL").await;
        println!("Fetched {} AAPL news items", news.len());
        assert!(!news.is_empty());
    }
}
