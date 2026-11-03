# システム構成図 (`terraform-gcp`)

`terraform-gcp/` の Terraform 定義から読み取れる Google Cloud および外部連携サービスのシステム構成図です。

## 全体アーキテクチャ図

```mermaid
flowchart TB
    %% クライアント・開発者
    User["ユーザー (Browser)"]
    Dev["開発者 / CI (Docker Build & Push)"]

    subgraph GCP["Google Cloud (asia-northeast1)"]
        
        subgraph Compute["Cloud Run"]
            FE["Cloud Run: Frontend (kinun-signal-frontend / Next.js :3000)"]
            BE["Cloud Run: Backend (kinun-signal-backend / Rust Axum :8080)"]
        end

        subgraph IAM["IAM Service Accounts"]
            FESA["Frontend SA (kinun-signal-frontend-sa)"]
            BESA["Backend SA (kinun-signal-backend-sa)"]
            SCSA["Scheduler SA (kinun-signal-scheduler-sa)"]
        end

        subgraph Batch["Cloud Scheduler"]
            JobDaily["daily-check (火〜土 06:30 JST)"]
            JobEarnings["earnings-check (火〜土 06:35 JST)"]
        end

        subgraph Security["Secret Manager"]
            SecFE["Basic認証シークレット (basic-auth-user / password)"]
            SecBE["アプリ連携シークレット (database-url / fmp / llm / line)"]
        end

        subgraph Registry["Artifact Registry"]
            AR["Docker Repository (kinun-signal-images: frontend / backend)"]
        end

        subgraph Ops["Cloud Monitoring & Logging"]
            Uptime["Uptime Check (3リージョン HTTPS GET /api/system/info)"]
            AlertPolicy["Alert Policy (2リージョン以上連続失敗検知)"]
            LogFilter["Cloud Logging Exclusion (正常ヘルスチェック 200 OK 除外)"]
        end
    end

    subgraph ExternalDB["Database (ap-northeast-1)"]
        Supabase[("Supabase PostgreSQL (Session Mode Pooler)")]
    end

    subgraph ExternalAPIs["外部API・通知連携"]
        FMP["Financial Modeling Prep API (株価・財務・決算・ニュース)"]
        LLM["LLM API (DCFパラメータ推論・要約)"]
        LINE["LINE Messaging API (割安・決算・要約通知)"]
        AlertDest["運用通知チャネル (Slack / Email)"]
    end

    %% ユーザー・開発者フロー
    User -->|"HTTPS (Basic認証 / allUsers run.invoker)"| FE
    Dev -->|"Docker Image Push"| AR
    AR -.->|"Image Pull"| FE
    AR -.->|"Image Pull"| BE

    %% サービス間通信・IAM権限
    FE -.->|"実行ID"| FESA
    BE -.->|"実行ID"| BESA
    FE -->|"内部プロキシ (/api/*) IAM認証 (run.invoker)"| BE

    %% バッチスケジューラ
    JobDaily -->|"POST /api/jobs/daily-check OIDC認証 (run.invoker)"| BE
    JobEarnings -->|"POST /api/jobs/earnings-check OIDC認証 (run.invoker)"| BE
    JobDaily -.->|"OIDCトークン発行"| SCSA
    JobEarnings -.->|"OIDCトークン発行"| SCSA

    %% シークレット参照
    FESA -->|"secretAccessor"| SecFE
    BESA -->|"secretAccessor"| SecBE

    %% バックエンド外部通信
    BE -->|"SQLx (PostgreSQL接続)"| Supabase
    BE -->|"HTTPS"| FMP
    BE -->|"HTTPS"| LLM
    BE -->|"HTTPS Push通知"| LINE

    %% 監視・ロギング
    Uptime -->|"HTTPS GET (Basic認証) /api/system/info"| FE
    Uptime -->|"メトリクス判定"| AlertPolicy
    AlertPolicy -->|"障害アラート発報"| AlertDest
    FE -.->|"アクセスログ出力"| LogFilter
```

---

## 認証・通信フロー詳細（IAM & セキュリティ境界）

```mermaid
sequenceDiagram
    autonumber
    actor User as ユーザー
    participant Uptime as Cloud Monitoring (Uptime Check)
    participant FE as Cloud Run: Frontend (frontend-sa)
    participant Scheduler as Cloud Scheduler (scheduler-sa)
    participant BE as Cloud Run: Backend (backend-sa)
    participant SM as Secret Manager
    participant Ext as Supabase / FMP / LLM / LINE

    Note over FE,SM: 起動時に Secret Manager から環境変数を注入
    FE->>SM: basic_auth_user / password 取得
    BE->>SM: database_url / 各種APIキー 取得

    rect rgb(240, 248, 255)
        Note over User,BE: 1. ユーザーアクセス（Webダッシュボード / BFF経由API）
        User->>FE: HTTPS リクエスト + Basic認証
        FE->>BE: バックエンドAPI呼び出し (Frontend SA の IAM OIDCトークン付与)
        BE->>Ext: DBクエリ / 外部API呼び出し
        Ext-->>BE: レスポンス
        BE-->>FE: JSONレスポンス
        FE-->>User: 画面描画 / JSON返却
    end

    rect rgb(245, 255, 245)
        Note over Scheduler,Ext: 2. 定期バッチ処理（日次チェック / 四半期決算チェック）
        Scheduler->>BE: POST /api/jobs/* (Scheduler SA の OIDCトークン付与)
        BE->>Ext: 株価・決算取得 / LLM推論 / DB更新 / LINE通知
        Ext-->>BE: 完了
        BE-->>Scheduler: 200 OK (実行サマリー)
    end

    rect rgb(255, 248, 240)
        Note over Uptime,BE: 3. エンドツーエンド死活監視
        Uptime->>FE: GET /api/system/info (Basic認証)
        FE->>BE: ヘルス＆システム情報確認
        BE-->>FE: 200 OK
        FE-->>Uptime: 200 OK
    end
```

---

## コンポーネント一覧（Terraformファイル対応表）

| カテゴリ | リソース / コンポーネント | 定義ファイル | 役割 |
| :--- | :--- | :--- | :--- |
| **Compute** | `kinun-signal-frontend` (Cloud Run) | `cloud_run.tf` | Next.js ダッシュボードおよびバックエンドへのプロキシ（BFF）。外部公開（Basic認証保護）。 |
| **Compute** | `kinun-signal-backend` (Cloud Run) | `cloud_run.tf` | Rust (Axum / SQLx) APIサーバー・DCF計算エンジン。IAM認証された内部呼び出しのみ許可。 |
| **Batch** | `kinun-signal-daily-check` (Cloud Scheduler) | `scheduler.tf` | 火〜土 06:30 JST に `/api/jobs/daily-check` を OIDC 認証で起動。 |
| **Batch** | `kinun-signal-earnings-check` (Cloud Scheduler) | `scheduler.tf` | 火〜土 06:35 JST に `/api/jobs/earnings-check` を OIDC 認証で起動。 |
| **IAM** | `frontend-sa` / `backend-sa` / `scheduler-sa` | `iam.tf` | 各コンポーネント専用のサービスアカウント。最小権限（`run.invoker`, `secretmanager.secretAccessor`）を付与。 |
| **Security** | `app_secrets` (Secret Manager) | `secrets.tf` | DB接続文字列、外部APIキー（FMP / LLM / LINE）、Basic認証情報を安全に管理。 |
| **Registry** | `kinun-signal-images` (Artifact Registry) | `artifact_registry.tf` | Frontend / Backend の Docker コンテナイメージを格納。 |
| **Monitoring** | Uptime Check / Alert Policy / Logging Exclusion | `monitoring.tf` | 3リージョンからの HTTPS 死活監視、連続失敗時の Slack / Email 通知、正常ヘルスチェックログの除外。 |
