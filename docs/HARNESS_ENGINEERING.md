# kinun-signal AI開発ハーネスエンジニアリング設計書

本ドキュメントは、`kinun-signal`（米国株・日本株対応の理論株価算出＆アラートシステム）において、マルチエージェント体制でトークン消費を抑えながら高品質な自律開発を継続するためのハーネスエンジニアリング仕様を定義します。

> **エントリーポイント**: 本設計書はルートの `AGENTS.md`（絶対遵守ルール）および `Makefile` の各ハーネスターゲットと直結しています。

---

## 1. モデルルーティングとツール呼出予算（Quota & Tool Call Budgeting）

AIエージェントは自身のリアルタイムな残トークン数を直接計測できないため、**「実際に機械的・行動的に守れる指標（モデル選択・1タスクあたりのツール呼出回数・1回あたりの読込行数）」** によってトークン消費を制御します。

### 1.1. 運用の3原則
- **安全バッファの温存**: 週次クォータの枯渇を防ぐため、1回の指示で巨大なリファクタリングを一括実行せず、1タスク＝1機能（または1バグ修正）の粒度に分割する。
- **軽量モデルの優先利用**: 実装・定型テストには高速・低コストな `flash` / `flash_lite` を優先し、複雑な設計判断やマルチファイル統合のみ `inherit` / `pro` を使用する。
- **探索コストの最小化**: ファイル探索時は全文読込を避け、`make agent-context-map` で行番号を特定してからピンポイントで読み込む。

### 1.2. ロール別モデル配分・行動制限マトリクス

| 階層 / ロール | 推奨モデル | 1回の読込上限 | ツール呼出目安 | 主な制約 |
| :--- | :--- | :--- | :--- | :--- |
| **Orchestrator** | `inherit` / `pro` | 最大 80 行 | $\le 3$ 回 / 委譲前 | 巨大ファイル直接読込禁止・`make agent-context-map` で事前インデックス化 |
| **Coder Subagent** | `flash` | 最大 80 行 | $\le 6$ 回 / タスク | 担当ディレクトリ外の Read/Write 禁止・差分のみ編集 |
| **QA / Gate Subagent** | `flash_lite` / `flash` | エラー先頭 30 行 | $\le 3$ 回 / 検証 | `make check` / `make smoke` の実行と失敗箇所の特定に専念 |

---

## 2. コンテキストダイエット・ハーネス（Context Slimming）

リポジトリ内の巨大ファイルやロックファイルを無防備に読み込むことによるコンテキスト汚染（Context Rot）とトークン浪費を防ぐため、以下の読み込みガードレールを適用します。

| 注意対象ファイル | サイズ | ハーネスによる読み込み制限ルール |
| :--- | :--- | :--- |
| `backend/src/services/fmp.rs` | **53.8 KB** | **全文読込禁止**。`make agent-context-map` で公開関数・構造体シグネチャと行番号を確認し、`StartLine`/`EndLine`（最大80行）でピンポイント読込 |
| `AGENTS.md` | **25.7 KB** | **サブエージェントの全文読込禁止**。`make agent-context-map` で章番号を確認するか、オーケストレーターが関連する仕様節（5〜20行）のみを委譲プロンプトに抜粋注入 |
| `backend/src/services/llm.rs` | **22.9 KB** | 行番号指定（80行以内）のみ許可。プロンプト文字列リテラル部分の不要な再読込を回避 |
| `backend/src/services/edinet.rs` / `jquants.rs` | **17.0 KB / 13.4 KB** | JPバックエンド担当エージェントのみ行範囲指定でアクセス可 |
| `Cargo.lock` / `package-lock.json` / `*.tfstate*` | **数百 KB** | **直接読込完全禁止**（依存調査は `cargo tree -i <crate>` または `npm why <pkg>` の出力先頭15行に限定） |

---

## 3. マルチエージェント体制とコンテキスト境界（File Ownership Matrix）

| エージェントロール | 推奨モデル | 書き込み許可 (Writable) | 読み込み許可 (Read-only・行範囲指定) | アクセス禁止 (Forbidden) |
| :--- | :--- | :--- | :--- | :--- |
| **Orchestrator** | `inherit` | `docs/**`, タスク管理ログ | `make agent-context-map` の出力、各設定ファイル | アプリケーション実装コードの直接大量編集 |
| **Backend-Agent** | `flash` | `backend/src/**`, `backend/migrations/**` | `backend/Cargo.toml`, `backend/clippy.toml` | `frontend/**`, `frontend-jp/**`, `*.lock` |
| **FrontendUS-Agent** | `flash` | `frontend/src/**`, `frontend/e2e/**` | `backend/src/models.rs` (US構造体のみ) | `backend/**` (書込), `frontend-jp/**`, `*.lock` |
| **FrontendJP-Agent** | `flash` | `frontend-jp/src/**` | `backend/src/models.rs` (JP構造体のみ), `backend/src/jp_handlers.rs` | `backend/**` (書込), `frontend/**`, `*.lock` |
| **QA & Infra-Agent** | `flash_lite` / `flash` | `Makefile`, `scripts/**`, `.github/**`, `terraform-gcp/**` | 各 `Dockerfile`, `package.json`, `Cargo.toml` | `*.tfstate*`, `.env`, アプリ内部ロジック |

### 最小トークン・ハンドオフ＆サーキットブレーカー（2-Strike Rule）
- **委譲プロンプト（Orchestrator $\to$ Subagent）**: 80語以内（対象ファイル、行範囲、制約、検証コマンドのみ）。
- **完了報告（Subagent $\to$ Orchestrator）**: 60語以内（変更ファイル一覧、検証コマンド終了コード、差分の要約のみ。コード全文の再出力は禁止）。
- **2-Strike サーキットブレーカー**: 同一エラーで検証（`make check` 等）が **2回連続失敗** した場合は自律リトライを停止し、エラー行と試行内容を簡潔にオーケストレーター（またはユーザー）へ返却してエスカレーションします。

---

## 4. 決定論的4層品質ゲートハーネス（`Makefile` 実装済み）

すべての変更は、以下の決定論的ゲートを通過しなければ完了とみなされません。

1. **L1: 静的解析・省略禁止・型検査（`make check-no-todo` / `make check-frontend` / `make check-frontend-jp`）**
   - `backend/src`, `frontend/src`, `frontend-jp/src` における `TODO`, `FIXME`, `unimplemented!()`, `todo!()` の完全禁止。
   - `cargo fmt --all -- --check` & `cargo clippy --all-targets -- -D warnings`
   - `frontend`（US版）および `frontend-jp`（JP版）双方の `npx tsc --noEmit` & `npm run build`
2. **L2: 単体テスト・DCF不変条件・モックフォールバック保証（`make check-backend`）**
   - `cargo test` によるDCF不変条件（`bull_price >= base_price >= bear_price >= 0.0`, `wacc > terminal_growth_rate`, ゼロ除算防止）の検証。
   - 外部APIキー未設定時の Graceful Fallback（FMP / J-Quants / EDINET / LLM / LINE モック動作）の保証。
3. **L3: US/JP APIスモークテスト＆契約検証（`make smoke` / `scripts/smoke_test.sh`）**
   - US版ルート（`/api/watchlist`, `/api/stocks/{ticker}`, etc.）＋JP版ルート（`/api/jp/watchlist`, `/api/jp/stocks/{code}`, `/api/jp/jobs/daily-check`, `/api/jp/jobs/earnings-check?force=true`, `/api/jp/alerts`）の `jq -e` 契約検証。
4. **L4: E2E・IaC・セキュリティ検証**
   - Playwright E2E（`frontend/e2e/dashboard.spec.ts`）、`terraform -chdir=terraform-gcp validate`、Semgrep / Trivy 検査。
