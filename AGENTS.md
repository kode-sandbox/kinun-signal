# AGENTS.md - 自律開発エージェント向け完全実装仕様書

あなたはシニアフルスタックエンジニア（Rust / Next.js専門）です。
本ドキュメントの仕様に100%従い、MacBook Air (Apple Silicon / localhost) 上で完全に動作する「米国株理論株価算出＆アラートシステム（Personal Stock Analyst）」をモノレポで実装してください。

## ⚠️ 絶対遵守ルール（Strict Constraints）
1. **省略の完全禁止**: `// TODO`, `// implement here`, `unimplemented!()` などの省略は一切禁止する。すべての関数・UIコンポーネントを最後まで完全に実装すること。
2. **コンパイル・ビルド保証**: バックエンドは `cargo test` および `cargo check` がエラーなく通ること。フロントエンドは `npm run build` が型エラーなく通ることを必ず確認してから作業を完了すること。
3. **Graceful Fallback（モック自動切替）の実装**: `.env` の `FMP_API_KEY`, `LLM_API_KEY`, `LINE_CHANNEL_ACCESS_TOKEN` が未設定・空文字・または `"your_..."` のような初期値である場合、エラーでクラッシュさせず、**自動的にリアルなダミーデータ（AAPLやNVDAの実際の財務規模に近い数値）を返して全フロー（DB保存・画面表示）を完走させるフォールバック処理**を各サービスクライアントに必ず実装すること。
4. **使用LLMモデル**: 環境変数 `LLM_MODEL`（デフォルト値: `argon`）を使用すること。

---

## 1. システム概要
ウォッチしている米国株の財務データを外部API（Financial Modeling Prep）から取得し、LLMを用いて将来の成長率・割引率を推論させた上で、Rust側で厳密なDCF法（資産価値＋事業価値）による理論株価を算出する。また、日次の終値監視による割安アラート、ニュースの日本語3行要約、四半期決算の速報と理論株価の自動再計算を行い、LINEへ通知する自分専用の証券マンアプリ。

---

## 2. ディレクトリ構造とインフラ定義

以下のツリー構造を正確に生成すること。

```text
.
├── AGENTS.md
├── .env
├── .env.example
├── docker-compose.yml
├── backend/
│   ├── Cargo.toml
│   ├── migrations/
│   │   └── 20261017000000_init.sql
│   └── src/
│       ├── main.rs
│       ├── config.rs
│       ├── db.rs
│       ├── models.rs
│       ├── dcf.rs
│       ├── handlers.rs
│       ├── scheduler.rs
│       └── services/
│           ├── mod.rs
│           ├── fmp.rs
│           ├── llm.rs
│           └── line.rs
└── frontend/
    ├── package.json
    ├── tsconfig.json
    ├── next.config.mjs
    ├── postcss.config.mjs
    ├── tailwind.config.ts
    └── src/
        ├── app/
        │   ├── globals.css
        │   ├── layout.tsx
        │   └── page.tsx
        ├── components/
        │   ├── Header.tsx
        │   ├── StockCard.tsx
        │   ├── StockDetailModal.tsx
        │   └── AddStockModal.tsx
        └── types/
            └── stock.ts
```

### 2.1. `docker-compose.yml`
```yaml
services:
  postgres:
    image: postgres:16-alpine
    container_name: stock_analyst_db
    environment:
      POSTGRES_USER: postgres
      POSTGRES_PASSWORD: password
      POSTGRES_DB: stock_analyst
    ports:
      - "5432:5432"
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres -d stock_analyst"]
      interval: 5s
      timeout: 5s
      retries: 5

volumes:
  pgdata:
```

### 2.2. `.env.example` 及び `.env`
開発開始時に `.env.example` と同じ内容で `.env` も生成しておくこと（そのままローカル起動できるようにするため）。

```env
DATABASE_URL=postgres://postgres:password@localhost:5432/stock_analyst
BACKEND_PORT=8080
NEXT_PUBLIC_API_URL=http://localhost:8080

# Financial Modeling Prep API
FMP_API_KEY=mock_fmp_key

# LLM API (OpenAI Chat Completions互換エンドポイント)
LLM_API_BASE_URL=[https://api.openai.com/v1](https://api.openai.com/v1)
LLM_API_KEY=mock_llm_key
LLM_MODEL=argon

# LINE Messaging API
LINE_CHANNEL_ACCESS_TOKEN=mock_line_token
LINE_USER_ID=mock_line_user_id
```

---

## 3. データベーススキーマ定義 (`backend/migrations/20261017000000_init.sql`)

※ SQLxのランタイムマイグレーション（`sqlx::migrate!("./migrations").run(&pool).await`）を `main.rs` 起動時に実行すること。また、コンパイル時のマクロ検査エラーを避けるため、クエリには `sqlx::query!` マクロではなく、非マクロの `sqlx::query` / `sqlx::query_as::<_, T>` (`#[derive(sqlx::FromRow)]`) を使用すること（DB未起動時でも `cargo check` が通るようにするため）。

```sql
CREATE TABLE IF NOT EXISTS watchlists (
    ticker VARCHAR(16) PRIMARY KEY,
    company_name VARCHAR(255) NOT NULL,
    target_discount_rate DOUBLE PRECISION NOT NULL DEFAULT 20.0,
    last_earnings_date DATE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS theoretical_prices (
    id SERIAL PRIMARY KEY,
    ticker VARCHAR(16) NOT NULL REFERENCES watchlists(ticker) ON DELETE CASCADE,
    trigger_type VARCHAR(50) NOT NULL, -- 'INITIAL', 'EARNINGS_UPDATE', 'MANUAL'
    base_price DOUBLE PRECISION NOT NULL,
    bull_price DOUBLE PRECISION NOT NULL,
    bear_price DOUBLE PRECISION NOT NULL,
    net_cash DOUBLE PRECISION NOT NULL,
    free_cash_flow DOUBLE PRECISION NOT NULL,
    shares_outstanding DOUBLE PRECISION NOT NULL,
    wacc DOUBLE PRECISION NOT NULL,
    growth_rate_base DOUBLE PRECISION NOT NULL,
    growth_rate_bull DOUBLE PRECISION NOT NULL,
    growth_rate_bear DOUBLE PRECISION NOT NULL,
    terminal_growth_rate DOUBLE PRECISION NOT NULL,
    reasoning_ja TEXT NOT NULL,
    calculated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE IF NOT EXISTS daily_prices (
    id SERIAL PRIMARY KEY,
    ticker VARCHAR(16) NOT NULL REFERENCES watchlists(ticker) ON DELETE CASCADE,
    trade_date DATE NOT NULL,
    close_price DOUBLE PRECISION NOT NULL,
    divergence_rate DOUBLE PRECISION NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(ticker, trade_date)
);

CREATE TABLE IF NOT EXISTS alert_logs (
    id SERIAL PRIMARY KEY,
    ticker VARCHAR(16) NOT NULL,
    alert_type VARCHAR(50) NOT NULL, -- 'UNDERVALUED', 'EARNINGS', 'NEWS_SUMMARY', 'INITIAL_CALC'
    message TEXT NOT NULL,
    sent_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

---

## 4. バックエンド詳細仕様 (`backend/`)

### 4.1. `backend/Cargo.toml`
依存クレートのバージョン競合を防ぐため、以下の構成を使用すること。

```toml
[package]
name = "stock_analyst_backend"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = { version = "0.7", features = ["macros"] }
tokio = { version = "1.38", features = ["full"] }
tower-http = { version = "0.5", features = ["cors", "trace"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "postgres", "chrono", "macros", "migrate"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
chrono = { version = "0.4", features = ["serde"] }
tokio-cron-scheduler = "0.10"
dotenvy = "0.15"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
anyhow = "1.0"
```

### 4.2. DCF法＋資産価値 計算アルゴリズム (`src/dcf.rs`)
以下の構造体と純粋関数を実装し、単体テストを必ず含めること。

```rust
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
```

#### 計算ルール
1. `shares_outstanding <= 0.0` の場合はゼロ除算を防ぐためエラーまたは全株価 `0.0` を返す。
2. `net_cash = cash_and_equivalents - total_debt`
3. 各シナリオ（`g` = `growth_rate_base`, `growth_rate_bull`, `growth_rate_bear`）について：
   * もし `free_cash_flow <= 0.0` の場合、事業価値（`ev`）は `0.0` とし、資産価値のみで計算する。
   * ガード処理: 割引率 `r = wacc.max(0.04)`。永久成長率 `g_t = terminal_growth_rate.min(r - 0.015).max(0.0)` （必ず `r > g_t` を保証する）。
   * 1年目〜5年目の割引FCF合計:
     `pv_fcf = (1..=5).map(|t| (free_cash_flow * (1.0 + g).powi(t)) / (1.0 + r).powi(t)).sum::<f64>()`
   * 5年目末のターミナルバリューとその現在価値:
     `fcf_5 = free_cash_flow * (1.0 + g).powi(5)`
     `tv = (fcf_5 * (1.0 + g_t)) / (r - g_t)`
     `pv_tv = tv / (1.0 + r).powi(5)`
   * 事業価値 `ev = pv_fcf + pv_tv`
   * 1株あたり理論株価 `price = ((net_cash + ev) / shares_outstanding).max(0.0)`
   * 小数点第2位で丸める: `(price * 100.0).round() / 100.0`

### 4.3. 外部APIクライアント仕様 (`src/services/`)

#### A. FMPクライアント (`services/fmp.rs`)
`FMP_API_KEY` が `"mock_fmp_key"` または空の場合は、通信を行わず以下のリアルなモックデータを返すこと（ティッカーに応じたバリエーションを持たせると良い）。実キーがある場合は `https://financialmodelingprep.com/api/v3` を叩くこと。
取得対象データ構造体：
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockFinancials {
    pub ticker: String,
    pub company_name: String,
    pub current_price: f64,
    pub cash_and_short_term_investments: f64,
    pub total_debt: f64,
    pub free_cash_flow: f64,        // 直近通期FCF
    pub fcf_history: Vec<f64>,      // 過去3期分のFCF推移
    pub revenue_history: Vec<f64>,  // 過去3期分の売上推移
    pub shares_outstanding: f64,
    pub latest_earnings_date: Option<chrono::NaiveDate>,
    pub eps_actual: Option<f64>,
    pub eps_estimated: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockNewsItem {
    pub title: String,
    pub published_date: String,
    pub text: String,
    pub url: String,
}
```
* **実APIエンドポイント**:
  * プロファイル: `GET /api/v3/profile/{ticker}?apikey={key}` (`companyName`, `price`)
  * BS: `GET /api/v3/balance-sheet-statement/{ticker}?limit=1&apikey={key}` (`cashAndShortTermInvestments`, `totalDebt`)
  * CF: `GET /api/v3/cash-flow-statement/{ticker}?limit=3&apikey={key}` (`freeCashFlow`)
  * PL: `GET /api/v3/income-statement/{ticker}?limit=3&apikey={key}` (`revenue`, `weightedAverageShsOutDil`)
  * 決算サプライズ: `GET /api/v3/earnings-surprises/{ticker}?apikey={key}` (`date`, `actualEarningResult`, `estimatedEarning`)
  * ニュース: `GET /api/v3/stock_news?tickers={ticker}&limit=5&apikey={key}` (`title`, `publishedDate`, `text`, `url`)

#### B. LLM クライアント (`services/llm.rs`)
* リクエスト先: `POST {LLM_API_BASE_URL}/chat/completions`
* ヘッダー: `Authorization: Bearer {LLM_API_KEY}`
* リクエストボディの `"model"` フィールドには環境変数 `LLM_MODEL` をセットすること。
* `LLM_API_KEY` が `"mock_llm_key"` または空の場合（およびAPI通信失敗時）は、財務データの過去FCF成長率から妥当な数値を自動算出したフォールバック値を返すこと。

**① DCFパラメータ推論プロンプト**:
* System Prompt:
  ```text
  あなたはウォール街のシニア株式アナリストです。提供された米国株の財務データと直近ニュースを分析し、DCF法（割引キャッシュフロー法）に必要な今後5年間のFCF成長率（基本・強気・弱気）、割引率（WACC）、永久成長率を推論してください。
  必ず以下のJSONフォーマットのみを出力してください。Markdownのコードブロック記号（```json）は含めないでください。
  {
    "wacc": 0.085,
    "growth_rate_base": 0.12,
    "growth_rate_bull": 0.18,
    "growth_rate_bear": 0.05,
    "terminal_growth_rate": 0.025,
    "reasoning_ja": "ここに日本語で300文字程度の算出根拠（財務健全性、FCFトレンド、競争優位性、リスク要因）を記述"
  }
  ```
* パース処理: レスポンス文字列から `serde_json::from_str` でパースする際、万が一LLMが ` ```json ... ``` ` で囲って返してきた場合でもパースできるよう、前後のバッククォートを除去するサニタイズ処理を入れること。

**② ニュース3行要約プロンプト**:
* System Prompt:
  ```text
  あなたは米国株の証券マンです。以下の英語ニュース群を読み、投資家向けに日本語で簡潔な「3行サマリー」と「株価への影響（ポジティブ / ネガティブ / 中立）」を作成してください。LINE通知で読みやすいよう、250文字以内にまとめてください。
  ```

#### C. LINE通知クライアント (`services/line.rs`)
* エンドポイント: `POST [https://api.line.me/v2/bot/message/push](https://api.line.me/v2/bot/message/push)`
* ヘッダー: `Authorization: Bearer {LINE_CHANNEL_ACCESS_TOKEN}`
* ボディ:
  ```json
  {
    "to": "{LINE_USER_ID}",
    "messages": [
      {
        "type": "text",
        "text": "送信メッセージ本文"
      }
    ]
  }
  ```
* `LINE_CHANNEL_ACCESS_TOKEN` が `"mock_line_token"` または空の場合は、標準出力（`tracing::info!`）にLINE通知内容を綺麗にフォーマットして出力し、DBの `alert_logs` テーブルに履歴を保存して正常終了とすること。

---

### 4.4. REST API エンドポイント仕様 (`src/handlers.rs`)
フロントエンド (`http://localhost:3000`) からのリクエストを受け付けるため、`tower_http::cors::CorsLayer::permissive()` を設定すること。

#### 1. `GET /api/watchlist`
ウォッチ銘柄一覧と、それぞれの最新理論株価・最新終値・乖離率を返す。
* **Response JSON (`200 OK`)**:
  ```json
  [
    {
      "ticker": "AAPL",
      "company_name": "Apple Inc.",
      "target_discount_rate": 15.0,
      "current_price": 225.50,
      "base_price": 240.00,
      "bull_price": 285.00,
      "bear_price": 195.00,
      "divergence_rate": -6.04,
      "is_undervalued_alert": false,
      "wacc": 0.085,
      "growth_rate_base": 0.11,
      "reasoning_ja": "強固なエコシステムと安定したFCF創出能力により...",
      "last_earnings_date": "2026-07-30",
      "calculated_at": "2026-10-17T08:00:00Z"
    }
  ]
  ```
  ※ `divergence_rate`（乖離率）の計算式: `((current_price - base_price) / base_price) * 100.0`
  （例: 理論株価200ドル、現在値150ドルの場合、乖離率は `-25.0` %）
  ※ `is_undervalued_alert` の判定式: `current_price <= base_price * (1.0 - target_discount_rate / 100.0)`

#### 2. `POST /api/watchlist`
銘柄を新規登録し、即座にFMPデータ取得 → LLM パラメータ推論 → DCF計算 → DB保存 → 初回LINE通知までを一気通貫で実行する。
* **Request JSON**:
  ```json
  {
    "ticker": "NVDA",
    "target_discount_rate": 20.0
  }
  ```
* **処理フロー**:
  1. `ticker` を大文字化・トリム。
  2. FMPから財務データ・現在株価・ニュースを取得。
  3. `watchlists` テーブルにINSERT（既存ならUPDATE）。
  4. LLM でDCFパラメータを推論。
  5. `dcf::calculate` で理論株価を算出し、`theoretical_prices` テーブルに `trigger_type = 'INITIAL'` でINSERT。
  6. `daily_prices` テーブルに当日終値と乖離率をINSERT（UPSERT）。
  7. LINEに「📊【新規銘柄登録＆理論株価算出】」を通知し、`alert_logs` に保存。
  8. 登録された銘柄の最新データを返す。

#### 3. `DELETE /api/watchlist/:ticker`
指定ティッカーを `watchlists` から削除する（CASCADEにより関連データも削除）。

#### 4. `GET /api/stocks/:ticker`
指定銘柄の詳細情報（過去の `theoretical_prices` 履歴リスト、直近の `daily_prices` リスト、直近の `alert_logs` リスト）を返す。
* **Response JSON (`200 OK`)**:
  ```json
  {
    "watchlist": { "ticker": "AAPL", "company_name": "Apple Inc.", "target_discount_rate": 15.0, "last_earnings_date": "2026-07-30" },
    "latest_theoretical": {
      "id": 1,
      "trigger_type": "INITIAL",
      "base_price": 240.0,
      "bull_price": 285.0,
      "bear_price": 195.0,
      "net_cash": -45000000000.0,
      "free_cash_flow": 105000000000.0,
      "shares_outstanding": 15400000000.0,
      "wacc": 0.085,
      "growth_rate_base": 0.11,
      "growth_rate_bull": 0.16,
      "growth_rate_bear": 0.06,
      "terminal_growth_rate": 0.025,
      "reasoning_ja": "...",
      "calculated_at": "2026-10-17T08:00:00Z"
    },
    "theoretical_history": [ ... ],
    "recent_alerts": [ ... ]
  }
  ```

#### 5. `POST /api/stocks/:ticker/recalculate`
理論株価の再計算を実行する。リクエストボディに手動オーバーライド用のパラメータが含まれている場合はそれを使用し、空（または省略）の場合はLLM に再推論させる。
* **Request JSON**:
  ```json
  {
    "custom_wacc": 0.09,
    "custom_growth_rate_base": 0.15
  }
  ```
  （※フィールドはすべて `Option<f64>` とし、`None` ならLLM再推論または前回値を使用）

#### 6. `POST /api/jobs/daily-check`
日次バッチ処理を手動でも即時発火できるエンドポイント。
* **処理内容**:
  1. 全ウォッチ銘柄を取得。
  2. 各銘柄の最新終値を取得し、`daily_prices` に保存。
  3. `終値 <= base_price * (1.0 - target_discount_rate / 100.0)` を満たす場合、LINEへ「🚨【割安アラート】{ticker} が理論株価({base_price}$)から{乖離率}%割安の {終値}$ に到達しました！」と通知。
  4. 各銘柄のニュースを取得し、LLM で日本語3行要約を作成してLINEへ「📰【モーニングニュース要約】」を通知。
  5. 実行結果サマリー（処理銘柄数、発火アラート数、生成されたメッセージ一覧）をJSONで返す。

#### 7. `POST /api/jobs/earnings-check`
四半期決算チェック処理を手動でも即時発火できるエンドポイント。
* **処理内容**:
  1. 全ウォッチ銘柄についてFMPの決算サプライズ情報を取得。
  2. `watchlists.last_earnings_date` と比較し、新しい決算日が存在する場合（またはテスト用に `force: true` クエリパラメータが渡された場合）：
     * 決算速報（EPS実績 vs 予想）をLINE通知。
     * 新しい財務データでLLM 推論 → DCF計算を再実行し、`trigger_type = 'EARNINGS_UPDATE'` で `theoretical_prices` に保存。
     * 新旧の理論株価比較をLINE通知し、`watchlists.last_earnings_date` を更新。
  3. 実行結果サマリーをJSONで返す。

#### 8. `GET /api/alerts`
直近の通知履歴（`alert_logs` テーブルの最新30件）を返す（Web画面上でLINE通知プレビューを確認できるようにするため）。

---

### 4.5. 定期スケジューラ仕様 (`src/scheduler.rs`)
`tokio-cron-scheduler` を用いて、バックエンド起動中に以下のバックグラウンドジョブを登録すること。
* 毎日 `0 30 21 * * Tue-Sat` (UTC 21:30 = 日本時間 翌朝 06:30、米国市場引け後):
  * `daily-check` 相当の処理と `earnings-check` 相当の処理を順番に自動実行する。

---

## 5. フロントエンド詳細仕様 (`frontend/`)

Next.js (App Router) + TypeScript + Tailwind CSS + `lucide-react` で実装する。
デザインテーマは**「Bloomberg / 機関投資家向けダークモード・ターミナル」**（背景: `bg-slate-950`, カード: `bg-slate-900 border-slate-800`, アクセント: エメラルドグリーン＆インディゴ）。

### 5.1. 画面構成 (`src/app/page.tsx`)
1画面で全ての状況把握と操作が完結するダッシュボードを構築すること。

1. **トップナビゲーションバー (`Header.tsx`)**:
   * サービスロゴ: `Personal Stock Analyst`
   * アクションボタン群（ローディング中はスピナーを表示しボタンを無効化すること）:
     * `＋ 銘柄を追加` ボタン（クリックで銘柄追加モーダルを開く）
     * `🔄 日次監視＆ニュース要約を実行` ボタン（`POST /api/jobs/daily-check` を叩き、完了トーストを表示）
     * `📊 決算チェック＆再計算を実行` ボタン（`POST /api/jobs/earnings-check?force=true` を叩き、完了トーストを表示）
2. **サマリーKPIバー**:
   * 監視中の銘柄数
   * 🎯 割安アラート点灯中の銘柄数（緑色で強調）
   * 最新のバッチ・通知実行時刻
3. **メインコンテンツ（2カラムレイアウト）**:
   * **左・中央カラム（幅 2/3）：ウォッチ銘柄カードグリッド (`StockCard.tsx`)**
     * 各銘柄のカードを表示。
     * 表示項目：ティッカー、企業名、現在株価、理論株価（Base / Bull / Bear の3バー表示）、現在値とBase理論株価の乖離率バッジ。
     * もし `is_undervalued_alert == true`（目標割安率以上に割安）なら、カードの枠線をエメラルドグリーンで光らせ、`🎯 買い圏内 (目標割安率達成)` バッジを表示。
     * LLM による算出根拠の先頭100文字プレビュー。
     * カードクリックで `StockDetailModal` を開く。
   * **右カラム（幅 1/3）：LINE通知＆アラート履歴タイムライン**
     * `GET /api/alerts` から取得した通知ログをLINEのトーク画面風のUIでリアルタイム表示する。
     * これにより、ローカル環境でLINE APIキーを設定する前でも、どんなLINE通知（割安アラート・日本語3行ニュース要約・決算速報）が飛んだかを画面上で一目で確認できる。
4. **銘柄詳細＆DCFシミュレーターモーダル (`StockDetailModal.tsx`)**:
   * LLM が生成した日本語の算出根拠レポート全文。
   * 財務基礎データ（ネットキャッシュ、直近FCF、発行済株式数）のフォーマット表示（十億ドル＝`$XX.XB` 表記）。
   * **インタラクティブDCF再計算機能**:
     * WACC（割引率）とBase成長率をスライダー（または数値入力）で変更し、「この前提で理論株価を再計算」ボタンを押すと `POST /api/stocks/:ticker/recalculate` を叩いてDBを更新できる機能。
   * 過去の理論株価算出履歴テーブル（初回算出 vs 決算後更新の推移）。
   * 銘柄削除ボタン。

---

## 6. 実装・検証の実行ステップ（Execution Checklist）

エージェントは以下の順序で作業を進め、各ステップを確実に完了させること。

- [ ] **Step 1**: ルートディレクトリの設定ファイル（`docker-compose.yml`, `.env.example`, `.env`）を作成する。
- [ ] **Step 2**: `backend/` ディレクトリを作成し、`Cargo.toml` と `migrations/20261017000000_init.sql` を配置する。
- [ ] **Step 3**: `backend/src/` 配下の全モジュール（`config.rs`, `models.rs`, `dcf.rs`, `db.rs`, `services/fmp.rs`, `services/llm.rs`, `services/line.rs`, `handlers.rs`, `scheduler.rs`, `main.rs`）を省略なしで実装する。
- [ ] **Step 4**: `cd backend && cargo test` を実行し、DCF計算ロジックの単体テストと全体のコンパイルがエラーなく通ることを確認する（警告やエラーが出た場合はこの段階で完全に修正しきる）。
- [ ] **Step 5**: `frontend/` ディレクトリを作成し、`package.json`, Tailwind設定、型定義 (`types/stock.ts`)、各コンポーネント、および `app/page.tsx` を実装する。
- [ ] **Step 6**: `cd frontend && npm install && npm run build` を実行し、TypeScriptの型エラーやビルドエラーが0件であることを確認する。
- [ ] **Step 7**: 初回起動時に画面が空で寂しくならないよう、バックエンド起動時のDB初期化処理（`db.rs`）において、もし `watchlists` テーブルが0件であれば、サンプルとして `AAPL` と `NVDA` の初期データを自動シード（DCF計算含む）する処理を実装しておく。

