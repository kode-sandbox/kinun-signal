use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::config::AppConfig;
use crate::dcf;
use crate::services::fmp::{StockFinancials, StockNewsItem};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmDcfParams {
    pub wacc: f64,
    pub growth_rate_base: f64,
    pub growth_rate_bull: f64,
    pub growth_rate_bear: f64,
    pub terminal_growth_rate: f64,
    pub reasoning_ja: String,
}

#[derive(Debug, Clone)]
pub struct LlmClient {
    http: Client,
    config: AppConfig,
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f64,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

impl LlmClient {
    pub fn new(config: AppConfig) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
            config,
        }
    }

    pub async fn infer_dcf_parameters(
        &self,
        financials: &StockFinancials,
        news: &[StockNewsItem],
    ) -> LlmDcfParams {
        if self.config.is_llm_mock() {
            info!(
                "LLM_API_KEY is mock/unset (model={}). Using autonomous CAPM WACC & DCF inference for {}",
                self.config.llm_model, financials.ticker
            );
            return Self::fallback_dcf_params(financials);
        }

        match self.call_llm_for_dcf(financials, news).await {
            Ok(params) => params,
            Err(err) => {
                warn!(
                    "LLM DCF inference failed for {} ({}). Using autonomous fallback parameters.",
                    financials.ticker, err
                );
                Self::fallback_dcf_params(financials)
            }
        }
    }

    async fn call_llm_for_dcf(
        &self,
        financials: &StockFinancials,
        news: &[StockNewsItem],
    ) -> anyhow::Result<LlmDcfParams> {
        let auto_ref = Self::fallback_dcf_params(financials);

        let system_prompt = r#"あなたはウォール街のシニア株式アナリストです。提供された米国株のYahoo Finance実財務データ・CAPM自走算出WACC・永久成長率・逆算DCF（市場織り込み成長率）・直近ニュースを分析し、10年3段階DCF法に必要な今後5年間のFCF成長率（基本・強気・弱気）、割引率（WACC）、永久成長率を推論してください。
WACCと永久成長率は、提供されたCAPM自走算出値（auto_wacc, auto_terminal_growth）を原則そのまま採用してください。
必ず以下のJSONフォーマットのみを出力してください。Markdownのコードブロック記号（```json）は含めないでください。
{
  "wacc": 0.085,
  "growth_rate_base": 0.12,
  "growth_rate_bull": 0.18,
  "growth_rate_bear": 0.05,
  "terminal_growth_rate": 0.028,
  "reasoning_ja": "ここに日本語で350文字程度の算出根拠（Yahoo Financeの財務数値、CAPMによるWACC算出根拠、永久成長率、市場織り込み成長率との比較、競争優位性・リスク要因）を記述"
}"#;

        let news_text = news
            .iter()
            .take(5)
            .map(|n| format!("- [{}] {}: {}", n.published_date, n.title, n.text))
            .collect::<Vec<_>>()
            .join("\n");

        let implied_g = dcf::calculate_implied_growth_rate(
            financials.current_price,
            financials.cash_and_short_term_investments,
            financials.total_debt,
            financials.free_cash_flow,
            financials.shares_outstanding,
            auto_ref.wacc,
            auto_ref.terminal_growth_rate,
        );

        let user_prompt = format!(
            "銘柄: {} ({})\n現在株価: ${:.2}\n現金及び短期投資: ${:.0}\n有利子負債合計: ${:.0}\n直近FCF(TTM/通期): ${:.0}\n過去FCF推移(新しい順): {:?}\n過去売上推移(新しい順): {:?}\n希薄化後発行済株式数: {:.0}\nBeta(Blume調整後): {:?}\n米10年国債利回り(Rf): {:?}\n実効税率: {:?}\nCAPM自走算出WACC (auto_wacc): {:.4}\n自走算出永久成長率 (auto_terminal_growth): {:.4}\n現在株価が織り込む逆算FCF成長率 (implied_growth_rate): {:?}\n\n直近ニュース:\n{}",
            financials.ticker,
            financials.company_name,
            financials.current_price,
            financials.cash_and_short_term_investments,
            financials.total_debt,
            financials.free_cash_flow,
            financials.fcf_history,
            financials.revenue_history,
            financials.shares_outstanding,
            financials.beta,
            financials.risk_free_rate,
            financials.effective_tax_rate,
            auto_ref.wacc,
            auto_ref.terminal_growth_rate,
            implied_g,
            news_text
        );

        let raw_content = self
            .send_chat_completion_with_model(&self.config.llm_model, system_prompt, &user_prompt)
            .await?;
        let cleaned = sanitize_json_response(&raw_content);
        let mut parsed: LlmDcfParams = serde_json::from_str(&cleaned)?;
        // Anchor WACC and Terminal Growth Rate to the self-calculated values if LLM deviates wildly
        parsed.wacc = auto_ref.wacc;
        parsed.terminal_growth_rate = auto_ref.terminal_growth_rate;
        Ok(parsed)
    }

    pub async fn summarize_news(
        &self,
        ticker: &str,
        company_name: &str,
        news: &[StockNewsItem],
    ) -> String {
        if self.config.is_llm_mock() {
            info!(
                "LLM_API_KEY is mock/unset (news_model={}). Generating structured Japanese 3-line summary for {}",
                self.config.llm_news_model, ticker
            );
            return Self::fallback_news_summary(ticker, company_name, news);
        }

        let system_prompt = "あなたは米国株の証券マンです。以下の英語ニュース群を読み、投資家向けに日本語で簡潔な「3行サマリー」と「株価への影響（ポジティブ / ネガティブ / 中立）」を作成してください。LINE通知で読みやすいよう、250文字以内にまとめてください。";

        let news_list = news
            .iter()
            .take(3)
            .map(|n| {
                let compact_text: String = n.text.chars().take(180).collect();
                format!(
                    "Title: {}\nDate: {}\nSummary: {}",
                    n.title, n.published_date, compact_text
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let user_prompt = format!(
            "銘柄: {} ({})\n\n英語ニュース一覧:\n{}",
            ticker, company_name, news_list
        );

        match self
            .send_chat_completion_with_model(
                &self.config.llm_news_model,
                system_prompt,
                &user_prompt,
            )
            .await
        {
            Ok(summary) if !summary.trim().is_empty() => summary.trim().to_string(),
            Err(err) => {
                warn!(
                    "LLM news summary (model={}) failed for {} ({}). Using fallback summary.",
                    self.config.llm_news_model, ticker, err
                );
                Self::fallback_news_summary(ticker, company_name, news)
            }
            _ => Self::fallback_news_summary(ticker, company_name, news),
        }
    }

    async fn send_chat_completion_with_model(
        &self,
        model_name: &str,
        system_prompt: &str,
        user_prompt: &str,
    ) -> anyhow::Result<String> {
        let url = format!("{}/chat/completions", self.config.llm_api_base_url);
        let resolved_model = if self
            .config
            .llm_api_base_url
            .contains("generativelanguage.googleapis.com")
            && (model_name.eq_ignore_ascii_case("argon")
                || model_name.eq_ignore_ascii_case("algon"))
        {
            "gemini-4-argon".to_string()
        } else {
            model_name.to_string()
        };
        let req_body = ChatCompletionRequest {
            model: resolved_model,
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system_prompt.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user_prompt.to_string(),
                },
            ],
            temperature: 0.2,
        };

        let resp = self
            .http
            .post(&url)
            .bearer_auth(&self.config.llm_api_key)
            .json(&req_body)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            anyhow::bail!("LLM HTTP {}: {}", status, err_text);
        }

        let body: ChatCompletionResponse = resp.json().await?;
        let text = body
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| anyhow::anyhow!("No choices returned from LLM"))?;

        Ok(text)
    }

    /// Yahoo Finance の実財務データから WACC・永久成長率・FCF成長率を自走算出する
    pub fn fallback_dcf_params(financials: &StockFinancials) -> LlmDcfParams {
        let latest_revenue = financials.revenue_history.first().copied().filter(|&r| r > 0.0);
        let fcf_cagr = compute_series_cagr(&financials.fcf_history);
        let rev_cagr = compute_series_cagr(&financials.revenue_history);

        // 1. CAPM + 資本構成から WACC を自走算出
        let wacc_comp = dcf::calculate_auto_wacc(
            financials.current_price,
            financials.shares_outstanding,
            financials.total_debt,
            financials.beta,
            financials.risk_free_rate,
            financials.interest_expense,
            financials.effective_tax_rate,
        );
        let wacc = wacc_comp.wacc;

        // 2. マクロ金利 + FCFマージン + 売上成長率から 永久成長率 を自走算出
        let terminal_growth_rate = dcf::calculate_auto_terminal_growth(
            wacc,
            financials.risk_free_rate,
            financials.free_cash_flow,
            latest_revenue,
            rev_cagr,
        );

        // 3. 市場株価が織り込んでいる暗黙の成長率 (Reverse DCF)
        let implied_g = dcf::calculate_implied_growth_rate(
            financials.current_price,
            financials.cash_and_short_term_investments,
            financials.total_debt,
            financials.free_cash_flow,
            financials.shares_outstanding,
            wacc,
            terminal_growth_rate,
        );

        // 4. 実績FCF成長率・売上成長率・収益性から基本成長率を推論
        let hist_blend = match (fcf_cagr, rev_cagr) {
            (Some(fg), Some(rg)) => {
                // 設備投資期でFCF成長率が一時的に売上成長率を下回る場合は売上成長率を重視
                if rg > fg {
                    (0.35 * fg.max(0.0) + 0.65 * rg).clamp(0.04, 0.36)
                } else {
                    (0.60 * fg + 0.40 * rg).clamp(0.04, 0.36)
                }
            }
            (Some(fg), None) => fg.clamp(0.04, 0.34),
            (None, Some(rg)) => rg.clamp(0.04, 0.30),
            (None, None) => 0.11,
        };

        // 単年度のFCF急増・急減ノイズを平滑化するため、ファンダメンタル成長率と市場コンセンサス水準を適正ブレンド
        let raw_base_g = if let Some(imp) = implied_g {
            let clamped_imp = imp.clamp(0.03, 0.38);
            (0.50 * hist_blend + 0.50 * clamped_imp).clamp(0.04, 0.35)
        } else {
            hist_blend
        };

        let growth_rate_base = dcf::round_three_decimals(raw_base_g);
        let spread = (growth_rate_base * 0.35).clamp(0.04, 0.09);
        let growth_rate_bull =
            dcf::round_three_decimals((growth_rate_base + spread).min(0.45));
        let growth_rate_bear =
            dcf::round_three_decimals((growth_rate_base - spread).max(0.01));

        let net_cash_b =
            (financials.cash_and_short_term_investments - financials.total_debt) / 1_000_000_000.0;
        let fcf_b = financials.free_cash_flow / 1_000_000_000.0;

        let mut details = Vec::new();
        details.push(format!(
            "直近FCF ${:.2}B・ネットキャッシュ {:+.2}B",
            fcf_b, net_cash_b
        ));
        if let Some(rev) = latest_revenue {
            if rev > 0.0 && financials.free_cash_flow > 0.0 {
                details.push(format!(
                    "売上高 ${:.2}B (FCFマージン {:.1}%)",
                    rev / 1e9,
                    (financials.free_cash_flow / rev) * 100.0
                ));
            }
        }
        if let Some(b) = wacc_comp.beta {
            details.push(format!("調整後β {:.2}", b));
        }
        details.push(format!(
            "米10年国債利回り {:.2}%",
            wacc_comp.risk_free_rate * 100.0
        ));
        if let Some(tax) = wacc_comp.effective_tax_rate {
            details.push(format!("実効税率 {:.1}%", tax * 100.0));
        }

        let implied_comment = match implied_g {
            Some(imp) => {
                let diff_pp = (imp - growth_rate_base) * 100.0;
                let market_view = if diff_pp > 3.0 {
                    "市場株価はファンダメンタル推計よりやや強気な成長期待を織り込んでいます。"
                } else if diff_pp < -3.0 {
                    "市場株価は実績キャッシュ創出力に対して保守的（割安）に評価されています。"
                } else {
                    "市場の織り込み成長率とファンダメンタル推計は概ね均衡しています。"
                };
                format!(
                    "逆算DCFによる市場織り込み成長率は年率 {:+.1}%（推計比 {:+.1}%pt）であり、{}",
                    imp * 100.0,
                    diff_pp,
                    market_view
                )
            }
            None => String::new(),
        };

        let reasoning_ja = format!(
            "【Yahoo Finance 実財務＆CAPM自走算出レポート】{} ({}) の取得データ（{}）に基づき、10年3段階フェードDCFモデルで評価しました。CAPM（株主資本コスト {:.1}%、株式比率 {:.1}%）と負債コストからWACCを {:.1}%、FCFマージンと長期金利から永久成長率を {:.1}% と自走算出。過去実績トレンドから今後5年間の基本FCF成長率を {:.1}%（強気 {:.1}% / 弱気 {:.1}%、6〜10年目は永久成長率へ段階的減衰）と推計しています。{}",
            financials.company_name,
            financials.ticker,
            details.join(" / "),
            wacc_comp.cost_of_equity * 100.0,
            wacc_comp.equity_weight * 100.0,
            wacc * 100.0,
            terminal_growth_rate * 100.0,
            growth_rate_base * 100.0,
            growth_rate_bull * 100.0,
            growth_rate_bear * 100.0,
            implied_comment
        );

        LlmDcfParams {
            wacc,
            growth_rate_base,
            growth_rate_bull,
            growth_rate_bear,
            terminal_growth_rate,
            reasoning_ja,
        }
    }

    pub fn fallback_news_summary(
        ticker: &str,
        _company_name: &str,
        news: &[StockNewsItem],
    ) -> String {
        if news.is_empty() {
            return format!(
                "① {} に関する直近の主要ニュースを監視中\n② Yahoo Finance の最新財務データに基づくDCFバリュエーションを維持\n③ 次回決算発表およびマクロ金利動向に注目\n💡 株価への影響：中立",
                ticker
            );
        }

        let lines: Vec<String> = news
            .iter()
            .take(3)
            .enumerate()
            .map(|(idx, item)| {
                let num = match idx {
                    0 => "①",
                    1 => "②",
                    _ => "③",
                };
                format!("{} {} ({})", num, item.title, item.published_date)
            })
            .collect();

        format!(
            "{}\n💡 株価への影響：ポジティブ（Yahoo Finance 最新ニュースヘッドラインより要約）",
            lines.join("\n")
        )
    }
}

/// 時系列データ（新しい順: index 0 が最新）から年平均成長率 (CAGR) を算出する
fn compute_series_cagr(series: &[f64]) -> Option<f64> {
    if series.len() < 2 {
        return None;
    }
    let latest = series[0];
    if latest <= 0.0 {
        return None;
    }

    // 最も古い正の値を探す（最大3期前まで）
    let max_idx = (series.len() - 1).min(3);
    for idx in (1..=max_idx).rev() {
        let older = series[idx];
        if older > 0.0 {
            let years = idx as f64;
            let cagr = (latest / older).powf(1.0 / years) - 1.0;
            if cagr.is_finite() {
                return Some(cagr);
            }
        }
    }
    None
}

pub fn sanitize_json_response(raw: &str) -> String {
    let trimmed = raw.trim();
    let without_prefix = if let Some(rest) = trimmed.strip_prefix("```json") {
        rest.trim()
    } else if let Some(rest) = trimmed.strip_prefix("```") {
        rest.trim()
    } else {
        trimmed
    };

    let without_suffix = if let Some(rest) = without_prefix.strip_suffix("```") {
        rest.trim()
    } else {
        without_prefix
    };

    if let (Some(start), Some(end)) = (without_suffix.find('{'), without_suffix.rfind('}')) {
        if start < end {
            return without_suffix[start..=end].to_string();
        }
    }

    without_suffix.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_json_response_strips_markdown_fences() {
        let fenced = "```json\n{\n  \"wacc\": 0.085,\n  \"growth_rate_base\": 0.12,\n  \"growth_rate_bull\": 0.18,\n  \"growth_rate_bear\": 0.05,\n  \"terminal_growth_rate\": 0.025,\n  \"reasoning_ja\": \"テスト根拠\"\n}\n```";
        let cleaned = sanitize_json_response(fenced);
        let parsed: LlmDcfParams =
            serde_json::from_str(&cleaned).expect("Should parse sanitized JSON");
        assert_eq!(parsed.wacc, 0.085);
        assert_eq!(parsed.growth_rate_base, 0.12);
    }

    #[test]
    fn test_fallback_dcf_uses_autonomous_wacc_and_terminal_growth() {
        let fin = StockFinancials {
            ticker: "AAPL".to_string(),
            company_name: "Apple Inc.".to_string(),
            current_price: 343.91,
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
            ],
            shares_outstanding: 14_714_676_000.0,
            latest_earnings_date: None,
            eps_actual: Some(2.02),
            eps_estimated: None,
            beta: Some(1.07),
            risk_free_rate: Some(0.044),
            effective_tax_rate: Some(0.156),
            interest_expense: Some(3_933_000_000.0),
        };

        let params = LlmClient::fallback_dcf_params(&fin);
        assert!(params.wacc >= 0.06 && params.wacc <= 0.12);
        assert!(params.terminal_growth_rate >= 0.02 && params.terminal_growth_rate <= 0.04);
        assert!(params.growth_rate_bull > params.growth_rate_base);
        assert!(params.growth_rate_base > params.growth_rate_bear);
        assert!(params.reasoning_ja.contains("CAPM"));
    }

    #[tokio::test]
    async fn test_live_yahoo_end_to_end_dcf_valuation() {
        let config = AppConfig::from_env();
        let fmp = crate::services::fmp::FmpClient::new(config);
        for ticker in ["AAPL", "NVDA", "MSFT", "GOOGL"] {
            let fin = fmp.fetch_financials(ticker).await;
            let params = LlmClient::fallback_dcf_params(&fin);
            let dcf_out = dcf::calculate(&dcf::DcfInput {
                cash_and_equivalents: fin.cash_and_short_term_investments,
                total_debt: fin.total_debt,
                free_cash_flow: fin.free_cash_flow,
                shares_outstanding: fin.shares_outstanding,
                wacc: params.wacc,
                growth_rate_base: params.growth_rate_base,
                growth_rate_bull: params.growth_rate_bull,
                growth_rate_bear: params.growth_rate_bear,
                terminal_growth_rate: params.terminal_growth_rate,
            });
            let div = dcf::calculate_divergence_rate(fin.current_price, dcf_out.base_price);
            let implied = dcf::calculate_implied_growth_rate(
                fin.current_price,
                fin.cash_and_short_term_investments,
                fin.total_debt,
                fin.free_cash_flow,
                fin.shares_outstanding,
                params.wacc,
                params.terminal_growth_rate,
            );
            println!(
                "[{}] Close=${:.2} | BaseDCF=${:.2} (Bull=${:.2}, Bear=${:.2}) | Div={:+.2}% | WACC={:.1}%, BaseG={:.1}%, TermG={:.1}% | ImpliedG={:?}",
                ticker,
                fin.current_price,
                dcf_out.base_price,
                dcf_out.bull_price,
                dcf_out.bear_price,
                div,
                params.wacc * 100.0,
                params.growth_rate_base * 100.0,
                params.terminal_growth_rate * 100.0,
                implied.map(|g| format!("{:.1}%", g * 100.0))
            );
            assert!(dcf_out.base_price > 0.0);
        }
    }
}
