#!/usr/bin/env bash
# ==============================================================================
# kinun-signal API Smoke Test Harness
# ==============================================================================
# Verifies end-to-end API contracts and Graceful Fallback behavior across all
# backend endpoints:
#   1. GET    /api/system/info
#   2. GET    /api/watchlist
#   3. POST   /api/watchlist
#   4. GET    /api/stocks/{ticker}
#   5. PATCH  /api/watchlist/{ticker}
#   6. POST   /api/stocks/{ticker}/recalculate
#   7. POST   /api/jobs/daily-check
#   8. POST   /api/jobs/earnings-check?force=true
#   9. GET    /api/alerts
#   10. DELETE /api/watchlist/{ticker}
# ==============================================================================

set -euo pipefail

API_URL="${API_URL:-http://localhost:8080}"
TEST_TICKER="${TEST_TICKER:-MSFT}"

echo "🚀 Starting API Smoke Test against ${API_URL} (Test Ticker: ${TEST_TICKER})..."

if ! command -v jq >/dev/null 2>&1; then
  echo "❌ Error: 'jq' is required for JSON contract assertions."
  exit 1
fi

fail() {
  echo "❌ SMOKE TEST FAILED: $1"
  exit 1
}

pass() {
  echo "✅ $1"
}

# Wait briefly for backend readiness (up to 15s)
for i in {1..15}; do
  if curl -sSf "${API_URL}/api/system/info" >/dev/null 2>&1; then
    break
  fi
  if [ "$i" -eq 15 ]; then
    fail "Backend at ${API_URL} is not reachable after 15 seconds."
  fi
  sleep 1
done

# 1. GET /api/system/info
SYS_INFO=$(curl -sSf "${API_URL}/api/system/info")
echo "${SYS_INFO}" | jq -e '.llm_model and .llm_news_model' >/dev/null \
  || fail "GET /api/system/info missing expected fields: ${SYS_INFO}"
pass "1/10 GET /api/system/info"

# 2. GET /api/watchlist
WATCHLIST=$(curl -sSf "${API_URL}/api/watchlist")
echo "${WATCHLIST}" | jq -e 'type == "array"' >/dev/null \
  || fail "GET /api/watchlist did not return an array"
pass "2/10 GET /api/watchlist (count: $(echo "${WATCHLIST}" | jq 'length'))"

# 3. POST /api/watchlist (Add test ticker & run DCF + fallback)
ADDED=$(curl -sSf -X POST "${API_URL}/api/watchlist" \
  -H "Content-Type: application/json" \
  -d "{\"ticker\": \"${TEST_TICKER}\", \"target_discount_rate\": 18.0}")
echo "${ADDED}" | jq -e \
  --arg ticker "${TEST_TICKER}" \
  '.ticker == $ticker and .base_price > 0 and .bull_price >= .base_price and .base_price >= .bear_price and (.reasoning_ja | length > 10)' >/dev/null \
  || fail "POST /api/watchlist invariant check failed: ${ADDED}"
pass "3/10 POST /api/watchlist (${TEST_TICKER} base_price=$(echo "${ADDED}" | jq -r '.base_price'))"

# 4. GET /api/stocks/{ticker}
DETAIL=$(curl -sSf "${API_URL}/api/stocks/${TEST_TICKER}")
echo "${DETAIL}" | jq -e \
  --arg ticker "${TEST_TICKER}" \
  '.watchlist.ticker == $ticker and .latest_theoretical.base_price > 0 and (.theoretical_history | length >= 1) and (.daily_prices | length >= 1)' >/dev/null \
  || fail "GET /api/stocks/${TEST_TICKER} contract check failed"
pass "4/10 GET /api/stocks/${TEST_TICKER}"

# 5. PATCH /api/watchlist/{ticker}
PATCHED=$(curl -sSf -X PATCH "${API_URL}/api/watchlist/${TEST_TICKER}" \
  -H "Content-Type: application/json" \
  -d '{"target_discount_rate": 25.0}')
echo "${PATCHED}" | jq -e '.target_discount_rate == 25.0' >/dev/null \
  || fail "PATCH /api/watchlist/${TEST_TICKER} did not update target_discount_rate: ${PATCHED}"
pass "5/10 PATCH /api/watchlist/${TEST_TICKER}"

# 6. POST /api/stocks/{ticker}/recalculate (Custom override)
RECALC=$(curl -sSf -X POST "${API_URL}/api/stocks/${TEST_TICKER}/recalculate" \
  -H "Content-Type: application/json" \
  -d '{"custom_wacc": 0.08, "custom_growth_rate_base": 0.14}')
echo "${RECALC}" | jq -e '.wacc == 0.08 and .growth_rate_base == 0.14 and .base_price > 0' >/dev/null \
  || fail "POST /api/stocks/${TEST_TICKER}/recalculate failed: ${RECALC}"
pass "6/10 POST /api/stocks/${TEST_TICKER}/recalculate (custom override)"

# 7. POST /api/jobs/daily-check
DAILY_JOB=$(curl -sSf -X POST "${API_URL}/api/jobs/daily-check")
echo "${DAILY_JOB}" | jq -e '.job_name == "daily-check" and .processed_count >= 1 and .alerts_triggered >= 1' >/dev/null \
  || fail "POST /api/jobs/daily-check failed: ${DAILY_JOB}"
pass "7/10 POST /api/jobs/daily-check (processed=$(echo "${DAILY_JOB}" | jq -r '.processed_count'))"

# 8. POST /api/jobs/earnings-check?force=true
EARNINGS_JOB=$(curl -sSf -X POST "${API_URL}/api/jobs/earnings-check?force=true")
echo "${EARNINGS_JOB}" | jq -e '.job_name == "earnings-check" and .processed_count >= 1 and .alerts_triggered >= 1' >/dev/null \
  || fail "POST /api/jobs/earnings-check?force=true failed: ${EARNINGS_JOB}"
pass "8/10 POST /api/jobs/earnings-check?force=true"

# 9. GET /api/alerts
ALERTS=$(curl -sSf "${API_URL}/api/alerts")
echo "${ALERTS}" | jq -e 'type == "array" and length >= 1' >/dev/null \
  || fail "GET /api/alerts returned empty or invalid array: ${ALERTS}"
pass "9/10 GET /api/alerts (logged alerts: $(echo "${ALERTS}" | jq 'length'))"

# 10. DELETE /api/watchlist/{ticker}
DELETED=$(curl -sSf -X DELETE "${API_URL}/api/watchlist/${TEST_TICKER}")
echo "${DELETED}" | jq -e --arg ticker "${TEST_TICKER}" '.deleted == true and .ticker == $ticker' >/dev/null \
  || fail "DELETE /api/watchlist/${TEST_TICKER} failed: ${DELETED}"
pass "10/16 DELETE /api/watchlist/${TEST_TICKER}"

# 11. GET /api/jp/watchlist
TEST_JP_CODE="${TEST_JP_CODE:-7203}"
JP_WATCHLIST=$(curl -sSf "${API_URL}/api/jp/watchlist")
echo "${JP_WATCHLIST}" | jq -e 'type == "array"' >/dev/null \
  || fail "GET /api/jp/watchlist did not return an array"
pass "11/16 GET /api/jp/watchlist (count: $(echo "${JP_WATCHLIST}" | jq 'length'))"

# 12. POST /api/jp/watchlist (Add JP stock & run DCF + fallback)
JP_ADDED=$(curl -sSf -X POST "${API_URL}/api/jp/watchlist" \
  -H "Content-Type: application/json" \
  -d "{\"code\": \"${TEST_JP_CODE}\", \"target_discount_rate\": 20.0}")
echo "${JP_ADDED}" | jq -e \
  --arg code "${TEST_JP_CODE}" \
  '.code == $code and .base_price > 0 and .bull_price >= .base_price and .base_price >= .bear_price' >/dev/null \
  || fail "POST /api/jp/watchlist invariant check failed: ${JP_ADDED}"
pass "12/16 POST /api/jp/watchlist (${TEST_JP_CODE} base_price=$(echo "${JP_ADDED}" | jq -r '.base_price'))"

# 13. GET /api/jp/stocks/{code}
JP_DETAIL=$(curl -sSf "${API_URL}/api/jp/stocks/${TEST_JP_CODE}")
echo "${JP_DETAIL}" | jq -e \
  --arg code "${TEST_JP_CODE}" \
  '.watchlist.code == $code and .latest_theoretical.base_price > 0' >/dev/null \
  || fail "GET /api/jp/stocks/${TEST_JP_CODE} contract check failed"
pass "13/16 GET /api/jp/stocks/${TEST_JP_CODE}"

# 14. POST /api/jp/jobs/daily-check & earnings-check?force=true
JP_DAILY=$(curl -sSf -X POST "${API_URL}/api/jp/jobs/daily-check")
echo "${JP_DAILY}" | jq -e '.processed_count >= 1' >/dev/null \
  || fail "POST /api/jp/jobs/daily-check failed: ${JP_DAILY}"
JP_EARNINGS=$(curl -sSf -X POST "${API_URL}/api/jp/jobs/earnings-check?force=true")
echo "${JP_EARNINGS}" | jq -e '.processed_count >= 1' >/dev/null \
  || fail "POST /api/jp/jobs/earnings-check?force=true failed: ${JP_EARNINGS}"
pass "14/16 POST /api/jp/jobs/daily-check & earnings-check?force=true"

# 15. GET /api/jp/alerts
JP_ALERTS=$(curl -sSf "${API_URL}/api/jp/alerts")
echo "${JP_ALERTS}" | jq -e 'type == "array" and length >= 1' >/dev/null \
  || fail "GET /api/jp/alerts returned empty or invalid array: ${JP_ALERTS}"
pass "15/16 GET /api/jp/alerts (logged alerts: $(echo "${JP_ALERTS}" | jq 'length'))"

# 16. DELETE /api/jp/watchlist/{code}
JP_DELETED=$(curl -sSf -X DELETE "${API_URL}/api/jp/watchlist/${TEST_JP_CODE}")
echo "${JP_DELETED}" | jq -e --arg code "${TEST_JP_CODE}" '.status == "deleted" and .code == $code' >/dev/null \
  || fail "DELETE /api/jp/watchlist/${TEST_JP_CODE} failed: ${JP_DELETED}"
pass "16/16 DELETE /api/jp/watchlist/${TEST_JP_CODE}"

echo "🎉 All 16 US & JP API smoke test assertions passed!"
