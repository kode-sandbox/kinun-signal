use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcfInput {
    pub cash_and_equivalents: f64, // 現金及び短期投資 (ドル)
    pub total_debt: f64,           // 有利子負債合計 (ドル)
    pub free_cash_flow: f64,       // 直近の基準FCF (ドル)
    pub shares_outstanding: f64,   // 希薄化後発行済株式数 (株)
    pub wacc: f64,                 // 割引率 (例: 0.085 = 8.5%)
    pub growth_rate_base: f64,     // 基本シナリオ成長率 (例: 0.12 = 12%)
    pub growth_rate_bull: f64,     // 強気シナリオ成長率 (例: 0.18 = 18%)
    pub growth_rate_bear: f64,     // 弱気シナリオ成長率 (例: 0.06 = 6%)
    pub terminal_growth_rate: f64, // 永久成長率 (例: 0.025 = 2.5%)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcfOutput {
    pub net_cash: f64,
    pub base_price: f64,
    pub bull_price: f64,
    pub bear_price: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaccComponents {
    pub wacc: f64,
    pub cost_of_equity: f64,
    pub after_tax_cost_of_debt: f64,
    pub equity_weight: f64,
    pub debt_weight: f64,
    pub beta: Option<f64>,
    pub risk_free_rate: f64,
    pub effective_tax_rate: Option<f64>,
}

pub fn calculate(input: &DcfInput) -> DcfOutput {
    let net_cash = input.cash_and_equivalents - input.total_debt;

    if input.shares_outstanding <= 0.0 {
        return DcfOutput {
            net_cash,
            base_price: 0.0,
            bull_price: 0.0,
            bear_price: 0.0,
        };
    }

    let base_price = calculate_scenario_price(
        net_cash,
        input.free_cash_flow,
        input.shares_outstanding,
        input.wacc,
        input.growth_rate_base,
        input.terminal_growth_rate,
    );
    let bull_price = calculate_scenario_price(
        net_cash,
        input.free_cash_flow,
        input.shares_outstanding,
        input.wacc,
        input.growth_rate_bull,
        input.terminal_growth_rate,
    );
    let bear_price = calculate_scenario_price(
        net_cash,
        input.free_cash_flow,
        input.shares_outstanding,
        input.wacc,
        input.growth_rate_bear,
        input.terminal_growth_rate,
    );

    DcfOutput {
        net_cash,
        base_price,
        bull_price,
        bear_price,
    }
}

/// 10年3段階フェードDCFモデル (3-Stage Fade DCF):
/// - 第1段階 (1〜5年目): シナリオ成長率 `g` でFCFが成長
/// - 第2段階 (6〜10年目): 成長率が `g` から永久成長率 `g_t` へ5年間で線形に減衰（フェード）
/// - 第3段階 (11年目以降): 10年目末時点のターミナルバリューを `g_t` で算出し現在価値へ割引
pub fn calculate_scenario_price(
    net_cash: f64,
    free_cash_flow: f64,
    shares_outstanding: f64,
    wacc: f64,
    g: f64,
    terminal_growth_rate: f64,
) -> f64 {
    if shares_outstanding <= 0.0 {
        return 0.0;
    }

    let ev = if free_cash_flow <= 0.0 {
        0.0
    } else {
        let r = wacc.max(0.04);
        let g_t = terminal_growth_rate.min(r - 0.015).max(0.0);

        let mut current_fcf = free_cash_flow;
        let mut pv_fcf = 0.0;

        // Stage 1: Years 1..=5 (High/Explicit Growth Phase)
        for t in 1..=5 {
            current_fcf *= 1.0 + g;
            pv_fcf += current_fcf / (1.0 + r).powi(t);
        }

        // Stage 2: Years 6..=10 (Linear Fade Phase from g to g_t)
        for step in 1..=5 {
            let t = 5 + step;
            let fade_ratio = (step as f64) / 5.0;
            let year_g = g + (g_t - g) * fade_ratio;
            current_fcf *= 1.0 + year_g;
            pv_fcf += current_fcf / (1.0 + r).powi(t);
        }

        // Stage 3: Terminal Value at Year 10
        let tv_10 = (current_fcf * (1.0 + g_t)) / (r - g_t);
        let pv_tv = tv_10 / (1.0 + r).powi(10);

        pv_fcf + pv_tv
    };

    let price = ((net_cash + ev) / shares_outstanding).max(0.0);
    round_two_decimals(price)
}

/// CAPM（資本資産価格モデル）と負債コスト・資本構成からWACCを自走算出する
pub fn calculate_auto_wacc(
    current_price: f64,
    shares_outstanding: f64,
    total_debt: f64,
    beta: Option<f64>,
    risk_free_rate: Option<f64>,
    interest_expense: Option<f64>,
    effective_tax_rate: Option<f64>,
) -> WaccComponents {
    // 米10年国債利回り (短期的な金利スパイクを平滑化するため長期均衡水準3.8%とブレンド)
    let raw_rf = risk_free_rate.unwrap_or(0.043).clamp(0.020, 0.070);
    let rf = 0.65 * raw_rf + 0.35 * 0.038;
    // 米国株式市場のインプライド株式リスクプレミアム (ERP: 4.1%)
    let erp = 0.041;
    // β値 (未取得時は 1.05 を使用、極端なボラティリティを抑制するため [0.55, 1.65] に制限)
    let b = beta.unwrap_or(1.05).clamp(0.55, 1.65);

    // 株主資本コスト Ke = Rf + β * ERP
    let cost_of_equity = rf + b * erp;

    let market_cap = (current_price * shares_outstanding).max(0.0);
    let debt = total_debt.max(0.0);

    // 税引前負債コスト Kd: 支払利息実績があれば (支払利息 / 有利子負債)、なければ Rf + 1.2%のスプレッド
    let pre_tax_cost_of_debt = if debt > 0.0 {
        if let Some(ie) = interest_expense {
            if ie > 0.0 {
                (ie / debt).clamp(rf + 0.004, rf + 0.055)
            } else {
                rf + 0.012
            }
        } else {
            rf + 0.012
        }
    } else {
        rf + 0.010
    };

    let tax_rate = effective_tax_rate.unwrap_or(0.21).clamp(0.0, 0.35);
    let after_tax_cost_of_debt = pre_tax_cost_of_debt * (1.0 - tax_rate);

    let total_capital = market_cap + debt;
    let (equity_weight, debt_weight) = if total_capital > 0.0 {
        (market_cap / total_capital, debt / total_capital)
    } else {
        (1.0, 0.0)
    };

    let raw_wacc = equity_weight * cost_of_equity + debt_weight * after_tax_cost_of_debt;
    let wacc = round_three_decimals(raw_wacc.clamp(0.055, 0.145));

    WaccComponents {
        wacc,
        cost_of_equity: round_three_decimals(cost_of_equity),
        after_tax_cost_of_debt: round_three_decimals(after_tax_cost_of_debt),
        equity_weight: round_three_decimals(equity_weight),
        debt_weight: round_three_decimals(debt_weight),
        beta: beta.map(round_two_decimals),
        risk_free_rate: round_three_decimals(raw_rf),
        effective_tax_rate: effective_tax_rate.map(|t| round_three_decimals(t.clamp(0.0, 0.40))),
    }
}

/// マクロ金利（米10年債利回り）と企業の収益性（FCFマージン・売上成長性）から永久成長率を自走算出する
pub fn calculate_auto_terminal_growth(
    wacc: f64,
    risk_free_rate: Option<f64>,
    free_cash_flow: f64,
    latest_revenue: Option<f64>,
    revenue_cagr: Option<f64>,
) -> f64 {
    // 長期名目GDP成長率アンカー（米10年債利回りの60%程度、2.0%〜3.0%の範囲）
    let rf = risk_free_rate.unwrap_or(0.043).clamp(0.020, 0.070);
    let mut gt = (rf * 0.58).clamp(0.020, 0.030);

    // FCFマージン（競争優位性・価格決定力の代理指標）による補正
    if let Some(rev) = latest_revenue {
        if rev > 0.0 && free_cash_flow > 0.0 {
            let fcf_margin = free_cash_flow / rev;
            if fcf_margin >= 0.28 {
                gt += 0.007;
            } else if fcf_margin >= 0.18 {
                gt += 0.004;
            } else if fcf_margin >= 0.10 {
                gt += 0.002;
            } else if fcf_margin < 0.05 {
                gt -= 0.004;
            }
        }
    }

    // 売上トレンドによる微調整
    if let Some(cagr) = revenue_cagr {
        if cagr >= 0.15 {
            gt += 0.003;
        } else if cagr >= 0.08 {
            gt += 0.001;
        } else if cagr < 0.0 {
            gt -= 0.003;
        }
    }

    // 永久成長率は経済合理性から最大4.0%、かつ WACC - 2.0% 以下に制限
    let max_allowed = (wacc - 0.020).min(0.040).max(0.015);
    round_three_decimals(gt.clamp(0.015, max_allowed))
}

/// 逆算DCF（Reverse DCF）: 現在の市場株価が織り込んでいる今後5年間の暗黙のFCF成長率 (Implied Growth Rate) を二分探索で算出する
pub fn calculate_implied_growth_rate(
    current_price: f64,
    cash_and_equivalents: f64,
    total_debt: f64,
    free_cash_flow: f64,
    shares_outstanding: f64,
    wacc: f64,
    terminal_growth_rate: f64,
) -> Option<f64> {
    if current_price <= 0.0 || free_cash_flow <= 0.0 || shares_outstanding <= 0.0 {
        return None;
    }

    let net_cash = cash_and_equivalents - total_debt;
    // もし資産価値だけで現在株価を超えている場合
    if net_cash / shares_outstanding >= current_price {
        return Some(-0.30);
    }

    let mut low = -0.35_f64;
    let mut high = 0.85_f64;

    for _ in 0..60 {
        let mid = 0.5 * (low + high);
        let model_price = calculate_scenario_price(
            net_cash,
            free_cash_flow,
            shares_outstanding,
            wacc,
            mid,
            terminal_growth_rate,
        );
        if model_price < current_price {
            low = mid;
        } else {
            high = mid;
        }
    }

    let implied = 0.5 * (low + high);
    Some(round_three_decimals(implied))
}

pub fn calculate_divergence_rate(current_price: f64, base_price: f64) -> f64 {
    if base_price <= 0.0 {
        return 0.0;
    }
    let rate = ((current_price - base_price) / base_price) * 100.0;
    round_two_decimals(rate)
}

pub fn is_undervalued(current_price: f64, base_price: f64, target_discount_rate: f64) -> bool {
    if base_price <= 0.0 {
        return false;
    }
    current_price <= base_price * (1.0 - target_discount_rate / 100.0)
}

pub fn round_two_decimals(val: f64) -> f64 {
    (val * 100.0).round() / 100.0
}

pub fn round_three_decimals(val: f64) -> f64 {
    (val * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dcf_standard_calculation() {
        let input = DcfInput {
            cash_and_equivalents: 65_000_000_000.0,
            total_debt: 105_000_000_000.0,
            free_cash_flow: 108_000_000_000.0,
            shares_outstanding: 15_200_000_000.0,
            wacc: 0.085,
            growth_rate_base: 0.11,
            growth_rate_bull: 0.16,
            growth_rate_bear: 0.06,
            terminal_growth_rate: 0.025,
        };

        let out = calculate(&input);
        assert_eq!(out.net_cash, -40_000_000_000.0);
        assert!(out.bull_price > out.base_price);
        assert!(out.base_price > out.bear_price);
        assert!(out.bear_price > 0.0);
    }

    #[test]
    fn test_dcf_zero_shares_guard() {
        let input = DcfInput {
            cash_and_equivalents: 1000.0,
            total_debt: 200.0,
            free_cash_flow: 500.0,
            shares_outstanding: 0.0,
            wacc: 0.08,
            growth_rate_base: 0.10,
            growth_rate_bull: 0.15,
            growth_rate_bear: 0.05,
            terminal_growth_rate: 0.02,
        };

        let out = calculate(&input);
        assert_eq!(out.base_price, 0.0);
        assert_eq!(out.bull_price, 0.0);
        assert_eq!(out.bear_price, 0.0);
    }

    #[test]
    fn test_dcf_negative_fcf_uses_asset_value_only() {
        let input = DcfInput {
            cash_and_equivalents: 5000.0,
            total_debt: 1000.0,
            free_cash_flow: -200.0,
            shares_outstanding: 100.0,
            wacc: 0.08,
            growth_rate_base: 0.10,
            growth_rate_bull: 0.20,
            growth_rate_bear: 0.05,
            terminal_growth_rate: 0.02,
        };

        let out = calculate(&input);
        assert_eq!(out.net_cash, 4000.0);
        assert_eq!(out.base_price, 40.0);
        assert_eq!(out.bull_price, 40.0);
        assert_eq!(out.bear_price, 40.0);
    }

    #[test]
    fn test_dcf_wacc_and_terminal_growth_guards() {
        let input = DcfInput {
            cash_and_equivalents: 1000.0,
            total_debt: 0.0,
            free_cash_flow: 100.0,
            shares_outstanding: 10.0,
            wacc: 0.01,                 // Should be clamped to 0.04
            growth_rate_base: 0.05,
            growth_rate_bull: 0.08,
            growth_rate_bear: 0.02,
            terminal_growth_rate: 0.10, // Should be clamped to r - 0.015 = 0.025
        };

        let out = calculate(&input);
        assert!(out.base_price.is_finite());
        assert!(out.base_price > 0.0);
    }

    #[test]
    fn test_auto_wacc_and_terminal_growth_and_reverse_dcf() {
        let wacc_comp = calculate_auto_wacc(
            225.0,
            15_000_000_000.0,
            100_000_000_000.0,
            Some(1.12),
            Some(0.044),
            Some(3_800_000_000.0),
            Some(0.16),
        );
        assert!(wacc_comp.wacc >= 0.055 && wacc_comp.wacc <= 0.145);
        assert!(wacc_comp.equity_weight > 0.90);

        let gt = calculate_auto_terminal_growth(
            wacc_comp.wacc,
            Some(0.044),
            110_000_000_000.0,
            Some(400_000_000_000.0),
            Some(0.09),
        );
        assert!(gt >= 0.015 && gt <= 0.040);

        let target_g = 0.125;
        let model_price = calculate_scenario_price(
            -35_000_000_000.0,
            110_000_000_000.0,
            15_000_000_000.0,
            wacc_comp.wacc,
            target_g,
            gt,
        );
        let implied_g = calculate_implied_growth_rate(
            model_price,
            65_000_000_000.0,
            100_000_000_000.0,
            110_000_000_000.0,
            15_000_000_000.0,
            wacc_comp.wacc,
            gt,
        )
        .expect("Should solve implied growth rate");
        assert!((implied_g - target_g).abs() < 0.002);
    }

    #[test]
    fn test_divergence_and_undervalued() {
        let div = calculate_divergence_rate(150.0, 200.0);
        assert_eq!(div, -25.0);
        assert!(is_undervalued(150.0, 200.0, 20.0));
        assert!(!is_undervalued(170.0, 200.0, 20.0));
    }
}
