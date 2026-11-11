//! EDINET API v2 client (Financial Services Agency) for the JP version.
//!
//! - Base: `https://disclosure.edinet-fsa.go.jp/api/v2`
//! - Auth: `Subscription-Key` header (env `EDINET_API_KEY`), free registration.
//! - Document list: GET /documents.json?date=YYYY-MM-DD&type=2
//! - Document body: GET /documents/{docID}?type=1 (XBRL zip) / ?type=5 (CSV)
//!
//! Role in the JP design: J-Quants supplies prices and the financial
//! *summary*, EDINET supplies the detailed BS/PL/CF figures the DCF needs
//! (cash, interest-bearing debt, operating CF, capex, shares outstanding).
//!
//! When the key is unset/mock, MOCK details are returned so the flow runs
//! end to end before registration.
//!
//! Known framework limits (deliberate, trial scope):
//! - XBRL zip archives are fetched as bytes but not yet unpacked (no zip
//!   reader wired in); extraction currently targets inline/plain XBRL XML
//!   text via `extract_dcf_details_from_xbrl`.
//! - The XBRL extractor matches element local-names and applies the
//!   `scale` attribute, but does not yet filter by contextRef
//!   (consolidated vs non-consolidated, instant vs duration) or normalise
//!   per-company taxonomy variants beyond the candidate lists below.
//!   Phase 0 acceptance requires hand-checking 7203 against its filing.

use chrono::NaiveDate;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::config::AppConfig;

const EDINET_BASE: &str = "https://disclosure.edinet-fsa.go.jp/api/v2";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdinetDocument {
    pub doc_id: String,
    pub edinet_code: Option<String>,
    pub sec_code: Option<String>,
    pub filer_name: Option<String>,
    pub doc_type_code: Option<String>,
    pub doc_description: Option<String>,
    pub submit_datetime: Option<String>,
}

/// Detailed statement figures extracted from EDINET XBRL, all in JPY.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdinetDcfDetails {
    pub cash_and_short_term_investments: f64,
    pub total_debt: f64,
    pub operating_cash_flow: f64,
    pub capex: f64, // positive amount spent
    pub free_cash_flow: f64, // operating CF - capex
    pub shares_outstanding: f64,
    pub revenue: Option<f64>,
    pub interest_expense: Option<f64>,
    pub effective_tax_rate: Option<f64>,
}

#[derive(Debug, Clone)]
pub struct EdinetClient {
    http: Client,
    config: AppConfig,
}

impl EdinetClient {
    pub fn new(config: AppConfig) -> Self {
        Self {
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(20))
                .build()
                .unwrap_or_default(),
            config,
        }
    }

    pub fn is_mock(&self) -> bool {
        self.config.is_edinet_mock()
    }

    /// Document metadata list for one day (type=2 = metadata only).
    pub async fn list_documents(&self, date: NaiveDate) -> anyhow::Result<Vec<EdinetDocument>> {
        if self.is_mock() {
            info!("EDINET_MOCK: returning mock document list for {date}");
            return Ok(vec![Self::mock_document()]);
        }
        let url = format!(
            "{EDINET_BASE}/documents.json?date={}&type=2",
            date.format("%Y-%m-%d")
        );
        let resp = self
            .http
            .get(&url)
            .header("Subscription-Key", &self.config.edinet_api_key)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("EDINET documents.json returned HTTP {}", resp.status());
        }
        let v: serde_json::Value = resp.json().await?;
        let docs = v
            .get("results")
            .and_then(|r| r.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|d| {
                        Some(EdinetDocument {
                            doc_id: d.get("docID")?.as_str()?.to_string(),
                            edinet_code: d.get("edinetCode").and_then(|x| x.as_str()).map(str::to_string),
                            sec_code: d.get("secCode").and_then(|x| x.as_str()).map(str::to_string),
                            filer_name: d.get("filerName").and_then(|x| x.as_str()).map(str::to_string),
                            doc_type_code: d
                                .get("docTypeCode")
                                .and_then(|x| x.as_str())
                                .map(str::to_string),
                            doc_description: d
                                .get("docDescription")
                                .and_then(|x| x.as_str())
                                .map(str::to_string),
                            submit_datetime: d
                                .get("submitDateTime")
                                .and_then(|x| x.as_str())
                                .map(str::to_string),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(docs)
    }

    /// Raw document body bytes (type=1: XBRL zip).
    pub async fn fetch_document_bytes(&self, doc_id: &str) -> anyhow::Result<Vec<u8>> {
        let url = format!("{EDINET_BASE}/documents/{doc_id}?type=1");
        let resp = self
            .http
            .get(&url)
            .header("Subscription-Key", &self.config.edinet_api_key)
            .send()
            .await?;
        if !resp.status().is_success() {
            anyhow::bail!("EDINET document {doc_id} returned HTTP {}", resp.status());
        }
        Ok(resp.bytes().await?.to_vec())
    }

    /// Find the filer's most recent securities/quarterly report and extract
    /// DCF details from it. Searches backwards day by day (bounded).
    pub async fn fetch_latest_dcf_details(
        &self,
        code: &str,
        _edinet_code: Option<&str>,
    ) -> anyhow::Result<EdinetDcfDetails> {
        if self.is_mock() {
            info!("EDINET_MOCK: returning mock DCF details for {code}");
            return Ok(Self::mock_details());
        }
        let today = chrono::Utc::now().date_naive();
        for offset in 0..120 {
            let date = today - chrono::Duration::days(offset);
            let docs = self.list_documents(date).await?;
            // docTypeCode 120 = annual securities report, 140 = quarterly.
            let hit = docs.iter().find(|d| {
                d.sec_code
                    .as_deref()
                    .map(|s| s.starts_with(code))
                    .unwrap_or(false)
                    && matches!(d.doc_type_code.as_deref(), Some("120") | Some("140"))
            });
            if let Some(doc) = hit {
                let bytes = self.fetch_document_bytes(&doc.doc_id).await?;
                if bytes.starts_with(b"PK") {
                    anyhow::bail!(
                        "EDINET document {} is a zip archive; zip extraction is not wired yet",
                        doc.doc_id
                    );
                }
                let xml = String::from_utf8_lossy(&bytes);
                return extract_dcf_details_from_xbrl(&xml).ok_or_else(|| {
                    anyhow::anyhow!("No DCF elements found in EDINET document {}", doc.doc_id)
                });
            }
        }
        anyhow::bail!("No EDINET securities/quarterly report found for {code} in the last 120 days")
    }

    fn mock_document() -> EdinetDocument {
        EdinetDocument {
            doc_id: "MOCK-EDINET-DOC-7203".to_string(),
            edinet_code: Some("E02144".to_string()),
            sec_code: Some("72030".to_string()),
            filer_name: Some("トヨタ自動車株式会社".to_string()),
            doc_type_code: Some("120".to_string()),
            doc_description: Some("有価証券報告書（モック）".to_string()),
            submit_datetime: Some("2026-06-25 09:00".to_string()),
        }
    }

    fn mock_details() -> EdinetDcfDetails {
        // Toyota-shaped figures (JPY). MOCK — for flow testing only.
        let operating_cf = 5_600_000_000_000.0;
        let capex = 2_300_000_000_000.0;
        EdinetDcfDetails {
            cash_and_short_term_investments: 10_500_000_000_000.0,
            total_debt: 27_000_000_000_000.0,
            operating_cash_flow: operating_cf,
            capex,
            free_cash_flow: operating_cf - capex,
            shares_outstanding: 13_140_000_000.0,
            revenue: Some(48_036_700_000_000.0),
            interest_expense: Some(320_000_000_000.0),
            effective_tax_rate: Some(0.28),
        }
    }
}

// ---- XBRL extraction layer ----

/// One element occurrence: its attributes string and raw text value.
fn element_values<'a>(xml: &'a str, local_name: &str) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for segment in xml.split('<').skip(1) {
        let Some(gt) = segment.find('>') else { continue };
        let tag_and_attrs = &segment[..gt];
        if tag_and_attrs.starts_with('/') || tag_and_attrs.ends_with('/') {
            continue; // closing or self-closing tag
        }
        let tag_name = tag_and_attrs
            .split(|c: char| c.is_whitespace())
            .next()
            .unwrap_or("");
        let local = tag_name.rsplit(':').next().unwrap_or(tag_name);
        if local != local_name {
            continue;
        }
        let raw_value = &segment[gt + 1..];
        let value_text = raw_value.split('<').next().unwrap_or("").trim();
        let cleaned: String = value_text.chars().filter(|c| *c != ',').collect();
        let Ok(mut num) = cleaned.parse::<f64>() else {
            continue;
        };
        // iXBRL/XBRL scale attribute: value x 10^scale.
        if let Some(scale) = attr_value(tag_and_attrs, "scale") {
            if let Ok(exp) = scale.parse::<i32>() {
                num *= 10f64.powi(exp);
            }
        }
        if attr_value(tag_and_attrs, "sign") == Some("-") {
            num = -num.abs();
        }
        out.push((tag_and_attrs.to_string(), num));
    }
    out
}

fn attr_value<'a>(attrs: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let start = attrs.find(&needle)? + needle.len();
    let rest = &attrs[start..];
    rest.find('"').map(|end| &rest[..end])
}

/// First occurrence (document order) of any candidate element.
fn first_of(xml: &str, candidates: &[&str]) -> Option<f64> {
    for c in candidates {
        if let Some((_, v)) = element_values(xml, c).into_iter().next() {
            return Some(v);
        }
    }
    None
}

/// Sum of first occurrences of several distinct elements (e.g. debt parts).
fn sum_of_firsts(xml: &str, candidates: &[&str]) -> Option<f64> {
    let mut total = 0.0;
    let mut found = false;
    for c in candidates {
        if let Some(v) = first_of(xml, &[c]) {
            total += v;
            found = true;
        }
    }
    if found {
        Some(total)
    } else {
        None
    }
}

/// Extract DCF inputs from an XBRL (or inline XBRL) document string.
/// Returns None when the essential elements (cash / FCF base / shares)
/// cannot be found at all.
pub fn extract_dcf_details_from_xbrl(xml: &str) -> Option<EdinetDcfDetails> {
    // Cash & short-term investments.
    let cash = match (
        first_of(xml, &["CashAndDeposits", "CashAndCashEquivalents"]),
        first_of(xml, &["ShortTermInvestmentSecurities", "MarketableSecurities"]),
    ) {
        (Some(c), Some(st)) => Some(c + st),
        (Some(c), None) => Some(c),
        _ => None,
    }?;

    // Interest-bearing debt: sum the standard borrowing/bond elements;
    // fall back to total liabilities when none are present.
    let total_debt = sum_of_firsts(
        xml,
        &[
            "ShortTermBorrowings",
            "ShortTermLoansPayable",
            "CurrentPortionOfBondsPayable",
            "CurrentPortionOfLongTermBorrowings",
            "CurrentPortionOfLongTermLoansPayable",
            "CommercialPapersIssued",
            "BondsPayable",
            "LongTermBorrowings",
            "LongTermLoansPayable",
        ],
    )
    .or_else(|| first_of(xml, &["TotalLiabilities"]))?;

    let operating_cf = first_of(xml, &[
        "NetCashProvidedByUsedInOperatingActivities",
        "CashFlowsFromOperatingActivities",
        "NetCashProvidedByOperatingActivities",
    ])?;
    let capex = sum_of_firsts(xml, &[
        "PurchaseOfPropertyPlantAndEquipment",
        "PaymentsForPurchaseOfPropertyPlantAndEquipment",
        "PurchaseOfIntangibleAssets",
    ])
    .unwrap_or(0.0);

    let issued = first_of(xml, &[
        "TotalNumberOfIssuedShares",
        "NumberOfSharesIssued",
        "IssuedSharesTotalQuantity",
    ])?;
    let treasury = first_of(xml, &["NumberOfTreasuryShares", "TreasuryShares"]).unwrap_or(0.0);
    let shares_outstanding = (issued - treasury).max(0.0);

    let revenue = first_of(xml, &[
        "NetSales",
        "RevenueFromContractsWithCustomers",
        "OperatingRevenue1",
        "OrdinaryIncome",
    ]);
    let interest_expense = first_of(xml, &["InterestExpenses", "InterestExpense"]);
    let effective_tax_rate = match (
        first_of(xml, &["IncomeTaxes", "IncomeTaxExpense", "TotalIncomeTaxes"]),
        first_of(xml, &["IncomeBeforeIncomeTaxes", "ProfitBeforeIncomeTaxes"]),
    ) {
        (Some(tax), Some(pre)) if pre > 0.0 => Some((tax / pre).clamp(0.0, 0.6)),
        _ => None,
    };

    Some(EdinetDcfDetails {
        cash_and_short_term_investments: cash,
        total_debt,
        operating_cash_flow: operating_cf,
        capex,
        free_cash_flow: operating_cf - capex,
        shares_outstanding,
        revenue,
        interest_expense,
        effective_tax_rate,
    })
}

#[cfg(test)]
mod tests {
    use super::extract_dcf_details_from_xbrl;

    // Small inline-XBRL-flavoured sample. Values use scale="6" (millions of
    // JPY), as real EDINET filings commonly do.
    const SAMPLE: &str = r#"
<xbrl xmlns:jppfs_cor="http://disclosure.edinet-fsa.go.jp/taxonomy/jppfs/2023-11-01/jppfs_cor">
  <jppfs_cor:CashAndDeposits contextRef="CurrentYearInstant" unitRef="JPY" scale="6">9000000</jppfs_cor:CashAndDeposits>
  <jppfs_cor:ShortTermInvestmentSecurities contextRef="CurrentYearInstant" unitRef="JPY" scale="6">1500000</jppfs_cor:ShortTermInvestmentSecurities>
  <jppfs_cor:ShortTermBorrowings contextRef="CurrentYearInstant" unitRef="JPY" scale="6">5000000</jppfs_cor:ShortTermBorrowings>
  <jppfs_cor:BondsPayable contextRef="CurrentYearInstant" unitRef="JPY" scale="6">3000000</jppfs_cor:BondsPayable>
  <jppfs_cor:LongTermBorrowings contextRef="CurrentYearInstant" unitRef="JPY" scale="6">12000000</jppfs_cor:LongTermBorrowings>
  <jppfs_cor:NetCashProvidedByUsedInOperatingActivities contextRef="CurrentYearDuration" unitRef="JPY" scale="6">5600000</jppfs_cor:NetCashProvidedByUsedInOperatingActivities>
  <jppfs_cor:PurchaseOfPropertyPlantAndEquipment contextRef="CurrentYearDuration" unitRef="JPY" scale="6">2300000</jppfs_cor:PurchaseOfPropertyPlantAndEquipment>
  <jppfs_cor:TotalNumberOfIssuedShares contextRef="CurrentYearInstant" unitRef="shares">16300000000</jppfs_cor:TotalNumberOfIssuedShares>
  <jppfs_cor:NumberOfTreasuryShares contextRef="CurrentYearInstant" unitRef="shares">3160000000</jppfs_cor:NumberOfTreasuryShares>
  <jppfs_cor:NetSales contextRef="CurrentYearDuration" unitRef="JPY" scale="6">48036700</jppfs_cor:NetSales>
</xbrl>"#;

    #[test]
    fn extracts_dcf_details_from_sample_xbrl() {
        let d = match extract_dcf_details_from_xbrl(SAMPLE) {
            Some(d) => d,
            None => {
                assert!(false, "extraction returned None for valid sample");
                return;
            }
        };
        // (9,000,000 + 1,500,000) million JPY
        assert_eq!(d.cash_and_short_term_investments, 10_500_000_000_000.0);
        // (5,000,000 + 3,000,000 + 12,000,000) million JPY
        assert_eq!(d.total_debt, 20_000_000_000_000.0);
        assert_eq!(d.operating_cash_flow, 5_600_000_000_000.0);
        assert_eq!(d.capex, 2_300_000_000_000.0);
        assert_eq!(d.free_cash_flow, 3_300_000_000_000.0);
        // 16.3B issued - 3.16B treasury
        assert_eq!(d.shares_outstanding, 13_140_000_000.0);
        assert_eq!(d.revenue, Some(48_036_700_000_000.0));
    }

    #[test]
    fn returns_none_when_essential_elements_missing() {
        let d = extract_dcf_details_from_xbrl("<xbrl><foo:Bar>1</foo:Bar></xbrl>");
        assert!(d.is_none());
    }
}
