-- Japanese-stock version tables (kinun-signal JP).
-- Kept fully separate from the US tables (watchlists / theoretical_prices /
-- daily_prices / alert_logs) so the two markets never share rows.
-- Provisional design (2026-11-12): expected to evolve after the trial.

CREATE TABLE IF NOT EXISTS jp_watchlists (
    code VARCHAR(8) PRIMARY KEY, -- TSE securities code, e.g. '7203'
    company_name VARCHAR(255) NOT NULL,
    edinet_code VARCHAR(16), -- EDINET filer code (e.g. 'E02144'); NULL until mapped
    target_discount_rate DOUBLE PRECISION NOT NULL DEFAULT 20.0,
    last_earnings_date DATE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS jp_theoretical_prices (
    id SERIAL PRIMARY KEY,
    code VARCHAR(8) NOT NULL REFERENCES jp_watchlists(code) ON DELETE CASCADE,
    trigger_type VARCHAR(50) NOT NULL, -- 'INITIAL', 'EARNINGS_UPDATE', 'MANUAL'
    base_price DOUBLE PRECISION NOT NULL, -- JPY
    bull_price DOUBLE PRECISION NOT NULL,
    bear_price DOUBLE PRECISION NOT NULL,
    net_cash DOUBLE PRECISION NOT NULL, -- JPY
    free_cash_flow DOUBLE PRECISION NOT NULL, -- JPY
    shares_outstanding DOUBLE PRECISION NOT NULL,
    wacc DOUBLE PRECISION NOT NULL,
    growth_rate_base DOUBLE PRECISION NOT NULL,
    growth_rate_bull DOUBLE PRECISION NOT NULL,
    growth_rate_bear DOUBLE PRECISION NOT NULL,
    terminal_growth_rate DOUBLE PRECISION NOT NULL,
    reasoning_ja TEXT NOT NULL,
    calculated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS jp_daily_prices (
    id SERIAL PRIMARY KEY,
    code VARCHAR(8) NOT NULL REFERENCES jp_watchlists(code) ON DELETE CASCADE,
    trade_date DATE NOT NULL,
    open_price DOUBLE PRECISION NOT NULL, -- JPY, unadjusted (J-Quants daily bars)
    high_price DOUBLE PRECISION NOT NULL,
    low_price DOUBLE PRECISION NOT NULL,
    close_price DOUBLE PRECISION NOT NULL,
    volume DOUBLE PRECISION NOT NULL,
    divergence_rate DOUBLE PRECISION NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(code, trade_date)
);

CREATE TABLE IF NOT EXISTS jp_alert_logs (
    id SERIAL PRIMARY KEY,
    code VARCHAR(8) NOT NULL,
    alert_type VARCHAR(50) NOT NULL, -- 'UNDERVALUED', 'EARNINGS', 'INITIAL_CALC'
    message TEXT NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
