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
    #[serde(rename = "reportedCurrency", default)]
    reported_currency: String,
    #[serde(rename = "cashAndShortTermInvestments", default)]
    cash_and_short_term_investments: f64,
    #[serde(rename = "totalDebt", default)]
    total_debt: f64,
}

#[derive(Debug, Deserialize)]
struct FmpCashFlowItem {
    #[serde(rename = "reportedCurrency", default)]
    reported_currency: String,
    #[serde(rename = "freeCashFlow", default)]
    free_cash_flow: f64,
    #[serde(rename = "operatingCashFlow", default)]
    operating_cash_flow: f64,
}

#[derive(Debug, Deserialize)]
struct FmpIncomeItem {
    #[serde(rename = "reportedCurrency", default)]
    reported_currency: String,
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
    currency_code: Option<String>,
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
        let mut ts_map = ts_res?;

        // ① 外国通貨建て財務データの米ドル（USD）自動換算（TSMのTWD、ASMLのEUR、SONYのJPY等）
        if let Some(fin_currency) = detect_reporting_currency(&ts_map) {
            if !fin_currency.eq_ignore_ascii_case("USD") {
                let fx_rate = self.fetch_fx_rate_to_usd(&fin_currency).await;
                if fx_rate > 0.0 && (fx_rate - 1.0).abs() > 1e-6 {
                    convert_timeseries_to_usd(&mut ts_map, fx_rate);
                    info!(
                        "Converted {} financial statement figures from {} to USD (rate: {:.4} {}/USD)",
                        symbol, fin_currency, fx_rate, fin_currency
                    );
                }
            }
        }

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

        // ② 基準FCFの算出と、一時的CapEx急増によるFCFマイナス／急減時の正規化補正（AMZN等のAIデータセンター投資期対応）
        let ttm_fcf_pt = latest_ts_point(&ts_map, "trailingFreeCashFlow");
        let annual_fcf_pts = ts_map.get("annualFreeCashFlow").cloned().unwrap_or_default();
        let ttm_ocf_pt = latest_ts_point(&ts_map, "trailingOperatingCashFlow");
        let annual_ocf_pts = ts_map
            .get("annualOperatingCashFlow")
            .cloned()
            .unwrap_or_default();

        let (free_cash_flow, fcf_history) = normalize_fcf_and_history(
            symbol,
            ttm_fcf_pt.as_ref(),
            &annual_fcf_pts,
            ttm_ocf_pt.as_ref(),
            &annual_ocf_pts,
        );

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
            .or_else(|| latest_ts_val(&ts_map, "annualDilutedAverageShares"))
            .or_else(|| latest_ts_val(&ts_map, "quarterlyOrdinarySharesNumber"))
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

    /// 指定された外国通貨（TWD, EUR, JPY, CNY, DKK 等）の対米ドル為替レート（1 USD あたりの現地通貨単位）を取得する
    pub async fn fetch_fx_rate_to_usd(&self, currency: &str) -> f64 {
        let raw_curr = currency.trim();
        if raw_curr.is_empty() || raw_curr.eq_ignore_ascii_case("USD") {
            return 1.0;
        }

        // GBp / GBX (British Pence: 100 pence = 1 GBP), ILA (Israeli Agorot: 100 agorot = 1 ILS)
        let (lookup_curr, subunit_multiplier) = if raw_curr == "GBp" || raw_curr.eq_ignore_ascii_case("GBX") {
            ("GBP".to_string(), 100.0)
        } else if raw_curr.eq_ignore_ascii_case("ILA") {
            ("ILS".to_string(), 100.0)
        } else {
            (raw_curr.to_uppercase(), 1.0)
        };

        let fx_symbol = format!("{}=X", lookup_curr);
        if let Ok((rate, _, _)) = self.fetch_chart_with_closes(&fx_symbol, "5d", "1d").await {
            if rate.is_finite() && rate > 0.0 {
                return rate * subunit_multiplier;
            }
        }

        let fallback = fallback_fx_rate_per_usd(&lookup_curr) * subunit_multiplier;
        warn!(
            "Yahoo FX quote failed for {}, using fallback rate {:.4} {}/USD",
            fx_symbol, fallback, raw_curr
        );
        fallback
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
            "trailingOperatingCashFlow",
            "annualOperatingCashFlow",
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
                let currency_code = item
                    .get("currencyCode")
                    .and_then(|c| c.as_str())
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let raw_val = item
                    .get("reportedValue")
                    .and_then(|rv| rv.get("raw"))
                    .and_then(|r| r.as_f64());

                if let Some(raw_value) = raw_val {
                    if raw_value.is_finite() {
                        points.push(TimeseriesPoint {
                            as_of_date,
                            raw_value,
                            currency_code,
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
        let cf_url = format!("{}/cash-flow-statement/{}?limit=4&apikey={}", base, symbol, key);
        let pl_url = format!("{}/income-statement/{}?limit=4&apikey={}", base, symbol, key);
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

        // ① 外国通貨建て財務諸表の米ドル換算
        let reported_curr = [&cf_list[0].reported_currency, &bs.reported_currency, &pl_list[0].reported_currency]
            .into_iter()
            .find(|c| !c.trim().is_empty())
            .cloned()
            .unwrap_or_else(|| "USD".to_string());
        let fx_rate = if !reported_curr.eq_ignore_ascii_case("USD") {
            self.fetch_fx_rate_to_usd(&reported_curr).await.max(1e-6)
        } else {
            1.0
        };

        // ② 一時的CapEx赤字時の基準FCF補正（営業CF・過去黒字FCF平均へのフォールバック）
        let (free_cash_flow, fcf_history) = normalize_fmp_api_fcf(symbol, &cf_list, fx_rate);
        let revenue_history = pl_list.iter().map(|p| p.revenue / fx_rate).collect();
        let shares_outstanding = pl_list[0].weighted_average_shs_out_dil;
        let interest_expense = pl_list[0].interest_expense.map(|ie| ie / fx_rate);
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
            cash_and_short_term_investments: bs.cash_and_short_term_investments / fx_rate,
            total_debt: bs.total_debt / fx_rate,
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

/// 財務諸表の金額項目（FCF・売上高・現金・負債等）から報告通貨コードを検出する
fn detect_reporting_currency(map: &HashMap<String, Vec<TimeseriesPoint>>) -> Option<String> {
    let monetary_keys = [
        "trailingFreeCashFlow",
        "annualFreeCashFlow",
        "trailingOperatingCashFlow",
        "annualOperatingCashFlow",
        "trailingTotalRevenue",
        "annualTotalRevenue",
        "quarterlyCashCashEquivalentsAndShortTermInvestments",
        "annualCashCashEquivalentsAndShortTermInvestments",
        "quarterlyTotalDebt",
        "annualTotalDebt",
    ];
    for key in monetary_keys {
        if let Some(pts) = map.get(key) {
            for pt in pts.iter().rev() {
                if let Some(curr) = &pt.currency_code {
                    if !curr.is_empty() {
                        return Some(curr.clone());
                    }
                }
            }
        }
    }
    None
}

/// 株式数・税率以外のすべての金額ベース時系列データを指定レート（現地通貨 / 1 USD）で米ドル換算する
fn convert_timeseries_to_usd(map: &mut HashMap<String, Vec<TimeseriesPoint>>, fx_rate_per_usd: f64) {
    if fx_rate_per_usd <= 0.0 || (fx_rate_per_usd - 1.0).abs() <= 1e-6 {
        return;
    }
    let non_monetary_keys = [
        "quarterlyDilutedAverageShares",
        "trailingDilutedAverageShares",
        "annualDilutedAverageShares",
        "quarterlyOrdinarySharesNumber",
        "annualTaxRateForCalcs",
    ];

    for (key, points) in map.iter_mut() {
        if non_monetary_keys.contains(&key.as_str()) {
            continue;
        }
        for pt in points.iter_mut() {
            pt.raw_value /= fx_rate_per_usd;
            pt.currency_code = Some("USD".to_string());
        }
    }
}

/// Yahoo Finance FX API が一時的に到達不能な場合のフォールバック為替レート（1 USD あたりの現地通貨単位）
fn fallback_fx_rate_per_usd(currency: &str) -> f64 {
    match currency.trim().to_uppercase().as_str() {
        "USD" => 1.0,
        "TWD" => 32.2,
        "JPY" => 155.0,
        "EUR" => 0.90,
        "GBP" => 0.76,
        "CNY" | "CNH" => 6.80,
        "HKD" => 7.82,
        "KRW" => 1350.0,
        "DKK" => 6.72,
        "CAD" => 1.39,
        "CHF" => 0.84,
        "INR" => 90.0,
        "BRL" => 5.30,
        "AUD" => 1.44,
        "SEK" => 10.0,
        "NOK" => 10.6,
        "SGD" => 1.30,
        "ILS" => 3.65,
        "MXN" => 19.0,
        "ZAR" => 18.0,
        "NZD" => 1.62,
        "THB" => 34.0,
        "IDR" => 15800.0,
        "MYR" => 4.35,
        "PHP" => 56.5,
        "PLN" => 3.95,
        "TRY" => 34.5,
        _ => 1.0,
    }
}

/// 基準FCFと過去FCF推移を構築し、AI設備投資（CapEx）急増等で直近FCFが一時的にマイナスまたは営業CF比で極端に圧縮されている場合は
/// 営業キャッシュフロー（OCF）および過去の黒字FCF実績に基づいて定常ベースラインFCFへ正規化する
fn normalize_fcf_and_history(
    symbol: &str,
    ttm_fcf_pt: Option<&TimeseriesPoint>,
    annual_fcf_pts: &[TimeseriesPoint],
    ttm_ocf_pt: Option<&TimeseriesPoint>,
    annual_ocf_pts: &[TimeseriesPoint],
) -> (f64, Vec<f64>) {
    // 1. 通常の基準FCF算出（TTMが通期より減少しているが正の場合は50/50ブレンド）
    let raw_fcf = match (ttm_fcf_pt, annual_fcf_pts.last()) {
        (Some(ttm), Some(ann)) => {
            if ttm.as_of_date >= ann.as_of_date {
                if ttm.raw_value > 0.0 && ann.raw_value > ttm.raw_value {
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

    let mut fcf_history: Vec<f64> = Vec::new();
    if let Some(ttm) = ttm_fcf_pt {
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

    // 2. 営業キャッシュフロー（OCF）の取得
    let latest_ocf = match (ttm_ocf_pt, annual_ocf_pts.last()) {
        (Some(ttm), Some(ann)) => {
            if ttm.as_of_date >= ann.as_of_date {
                ttm.raw_value.max(ann.raw_value)
            } else {
                ann.raw_value
            }
        }
        (Some(ttm), None) => ttm.raw_value,
        (None, Some(ann)) => ann.raw_value,
        (None, None) => 0.0,
    };

    // 3. 一時的CapEx急増によるFCFマイナス／急減の判定
    // - raw_fcf <= 0.0 (AMZN等、直近TTMのAI設備投資でFCFが一時的にマイナス転落したケース)
    // - または 営業CFが黒字かつ raw_fcf が営業CFの15%未満に急減しているケース
    let is_capex_depressed =
        raw_fcf <= 0.0 || (latest_ocf > 0.0 && raw_fcf < latest_ocf * 0.15);

    if !is_capex_depressed {
        return (raw_fcf, fcf_history);
    }

    // 4. 過去の正常な黒字FCF実績と、正常期の (FCF / 営業CF) 転換率を集計
    let mut ocf_by_date: HashMap<&str, f64> = HashMap::new();
    for pt in annual_ocf_pts {
        ocf_by_date.insert(pt.as_of_date.as_str(), pt.raw_value);
    }

    let mut healthy_pos_fcfs = Vec::new();
    let mut all_pos_fcfs = Vec::new();
    let mut healthy_conversion_ratios = Vec::new();

    for pt in annual_fcf_pts {
        if pt.raw_value > 0.0 {
            all_pos_fcfs.push(pt.raw_value);
            if let Some(&yr_ocf) = ocf_by_date.get(pt.as_of_date.as_str()) {
                if yr_ocf > 0.0 {
                    let ratio = pt.raw_value / yr_ocf;
                    if ratio >= 0.15 {
                        healthy_pos_fcfs.push(pt.raw_value);
                        healthy_conversion_ratios.push(ratio.clamp(0.20, 0.75));
                    }
                } else {
                    healthy_pos_fcfs.push(pt.raw_value);
                }
            } else {
                healthy_pos_fcfs.push(pt.raw_value);
            }
        }
    }

    let hist_pos_fcf_avg = if !healthy_pos_fcfs.is_empty() {
        healthy_pos_fcfs.iter().sum::<f64>() / (healthy_pos_fcfs.len() as f64)
    } else if !all_pos_fcfs.is_empty() {
        all_pos_fcfs.iter().sum::<f64>() / (all_pos_fcfs.len() as f64)
    } else {
        0.0
    };

    let hist_conversion_ratio = if !healthy_conversion_ratios.is_empty() {
        let avg_r = healthy_conversion_ratios.iter().sum::<f64>()
            / (healthy_conversion_ratios.len() as f64);
        // 過去の正常期転換率と定常メンテナンスCapEx控除後転換率(40%)をブレンド
        (0.50 * avg_r + 0.50 * 0.40).clamp(0.32, 0.50)
    } else {
        0.38
    };

    let ocf_based_fcf = if latest_ocf > 0.0 {
        latest_ocf * hist_conversion_ratio
    } else {
        0.0
    };

    let normalized_fcf = match (ocf_based_fcf > 0.0, hist_pos_fcf_avg > 0.0) {
        (true, true) => {
            // 直近の営業CF規模を反映した正規化FCF(80%)と過去の黒字FCF平均(20%)をブレンド
            if ocf_based_fcf >= hist_pos_fcf_avg {
                0.80 * ocf_based_fcf + 0.20 * hist_pos_fcf_avg
            } else {
                0.50 * ocf_based_fcf + 0.50 * hist_pos_fcf_avg
            }
        }
        (true, false) => ocf_based_fcf,
        (false, true) => hist_pos_fcf_avg,
        (false, false) => raw_fcf,
    };

    if normalized_fcf <= 0.0 {
        return (raw_fcf, fcf_history);
    }

    info!(
        "Normalized temporary CapEx-depressed FCF for {}: raw_fcf=${:.2}B -> normalized_fcf=${:.2}B (latest_ocf=${:.2}B, hist_pos_fcf_avg=${:.2}B)",
        symbol,
        raw_fcf / 1e9,
        normalized_fcf / 1e9,
        latest_ocf / 1e9,
        hist_pos_fcf_avg / 1e9
    );

    // 5. fcf_history も正規化（CAGR算出やLLMプロンプトでマイナスFCFによる破綻を防ぐ）
    let eff_ratio = if latest_ocf > 0.0 {
        (normalized_fcf / latest_ocf).clamp(0.25, 0.55)
    } else {
        hist_conversion_ratio
    };

    let mut normalized_history = Vec::new();
    normalized_history.push(normalized_fcf);
    for pt in annual_fcf_pts.iter().rev() {
        let yr_ocf = ocf_by_date.get(pt.as_of_date.as_str()).copied().unwrap_or(0.0);
        if pt.raw_value > 0.0 && (yr_ocf <= 0.0 || pt.raw_value >= yr_ocf * 0.15) {
            normalized_history.push(pt.raw_value);
        } else if yr_ocf > 0.0 {
            normalized_history.push(yr_ocf * eff_ratio);
        } else if hist_pos_fcf_avg > 0.0 {
            normalized_history.push(hist_pos_fcf_avg);
        }
    }

    (normalized_fcf, normalized_history)
}

/// FMP API 経由のキャッシュフロー計算書に対しても同様の通貨換算および一時的CapEx赤字時のFCF正規化を適用する
fn normalize_fmp_api_fcf(
    symbol: &str,
    cf_list: &[FmpCashFlowItem],
    fx_rate: f64,
) -> (f64, Vec<f64>) {
    let annual_fcf_pts: Vec<TimeseriesPoint> = cf_list
        .iter()
        .rev()
        .enumerate()
        .map(|(idx, c)| TimeseriesPoint {
            as_of_date: format!("FY{}", idx),
            raw_value: c.free_cash_flow / fx_rate,
            currency_code: Some("USD".to_string()),
        })
        .collect();
    let annual_ocf_pts: Vec<TimeseriesPoint> = cf_list
        .iter()
        .rev()
        .enumerate()
        .filter(|(_, c)| c.operating_cash_flow != 0.0)
        .map(|(idx, c)| TimeseriesPoint {
            as_of_date: format!("FY{}", idx),
            raw_value: c.operating_cash_flow / fx_rate,
            currency_code: Some("USD".to_string()),
        })
        .collect();

    normalize_fcf_and_history(symbol, None, &annual_fcf_pts, None, &annual_ocf_pts)
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

        // ① TSM: TWD建て財務諸表が米ドル(USD)に正しく換算されていることを検証
        let tsm_fin = client.fetch_financials("TSM").await;
        println!(
            "Fetched TSM Price: ${:.2}, FCF(USD): ${:.2}B, Cash(USD): ${:.2}B, Debt(USD): ${:.2}B, Shares: {:.2}B",
            tsm_fin.current_price,
            tsm_fin.free_cash_flow / 1e9,
            tsm_fin.cash_and_short_term_investments / 1e9,
            tsm_fin.total_debt / 1e9,
            tsm_fin.shares_outstanding / 1e9
        );
        // 台湾ドル未換算ならFCFは約1.12兆(1124B)になるが、米ドル換算後は約$20B〜$60Bの範囲に収まる
        assert!(tsm_fin.free_cash_flow > 10_000_000_000.0);
        assert!(tsm_fin.free_cash_flow < 150_000_000_000.0);

        // ② AMZN: AI設備投資による一時的TTM FCFマイナスが営業CF・過去黒字FCFベースで正の基準FCFへ補正されることを検証
        let amzn_fin = client.fetch_financials("AMZN").await;
        println!(
            "Fetched AMZN Price: ${:.2}, Normalized FCF(USD): ${:.2}B, FCF History: {:?}",
            amzn_fin.current_price,
            amzn_fin.free_cash_flow / 1e9,
            amzn_fin.fcf_history.iter().map(|v| format!("${:.1}B", v / 1e9)).collect::<Vec<_>>()
        );
        assert!(amzn_fin.free_cash_flow > 20_000_000_000.0);
    }

    #[test]
    fn test_foreign_currency_conversion_and_capex_fcf_normalization() {
        // 1. TWD -> USD 換算の単体テスト
        let mut ts_map: HashMap<String, Vec<TimeseriesPoint>> = HashMap::new();
        ts_map.insert(
            "trailingFreeCashFlow".to_string(),
            vec![TimeseriesPoint {
                as_of_date: "2026-06-30".to_string(),
                raw_value: 1_124_666_937_000.0,
                currency_code: Some("TWD".to_string()),
            }],
        );
        ts_map.insert(
            "quarterlyDilutedAverageShares".to_string(),
            vec![TimeseriesPoint {
                as_of_date: "2026-06-30".to_string(),
                raw_value: 5_186_400_000.0,
                currency_code: Some("TWD".to_string()),
            }],
        );

        assert_eq!(detect_reporting_currency(&ts_map).as_deref(), Some("TWD"));
        convert_timeseries_to_usd(&mut ts_map, 32.0);
        let usd_fcf = latest_ts_val(&ts_map, "trailingFreeCashFlow").unwrap();
        let shares = latest_ts_val(&ts_map, "quarterlyDilutedAverageShares").unwrap();
        assert!((usd_fcf - 35_145_841_781.25).abs() < 1.0);
        // 株式数は通貨換算で割られないことを確認
        assert_eq!(shares, 5_186_400_000.0);

        // 2. AMZN型の一時的CapEx赤字時のFCF正規化テスト
        let ttm_fcf = TimeseriesPoint {
            as_of_date: "2026-06-30".to_string(),
            raw_value: -11_625_000_000.0,
            currency_code: Some("USD".to_string()),
        };
        let annual_fcfs = vec![
            TimeseriesPoint {
                as_of_date: "2023-12-31".to_string(),
                raw_value: 32_217_000_000.0,
                currency_code: Some("USD".to_string()),
            },
            TimeseriesPoint {
                as_of_date: "2024-12-31".to_string(),
                raw_value: 32_878_000_000.0,
                currency_code: Some("USD".to_string()),
            },
            TimeseriesPoint {
                as_of_date: "2025-12-31".to_string(),
                raw_value: 7_695_000_000.0,
                currency_code: Some("USD".to_string()),
            },
        ];
        let ttm_ocf = TimeseriesPoint {
            as_of_date: "2026-06-30".to_string(),
            raw_value: 161_403_000_000.0,
            currency_code: Some("USD".to_string()),
        };
        let annual_ocfs = vec![
            TimeseriesPoint {
                as_of_date: "2023-12-31".to_string(),
                raw_value: 84_946_000_000.0,
                currency_code: Some("USD".to_string()),
            },
            TimeseriesPoint {
                as_of_date: "2024-12-31".to_string(),
                raw_value: 115_877_000_000.0,
                currency_code: Some("USD".to_string()),
            },
            TimeseriesPoint {
                as_of_date: "2025-12-31".to_string(),
                raw_value: 139_514_000_000.0,
                currency_code: Some("USD".to_string()),
            },
        ];

        let (norm_fcf, norm_hist) = normalize_fcf_and_history(
            "AMZN",
            Some(&ttm_fcf),
            &annual_fcfs,
            Some(&ttm_ocf),
            &annual_ocfs,
        );
        assert!(norm_fcf > 45_000_000_000.0 && norm_fcf < 75_000_000_000.0);
        assert!(norm_hist.iter().all(|&v| v > 0.0));
    }
}
