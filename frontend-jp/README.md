# kinun-signal JP フロントエンド（日本株版）

米国版 `frontend/` から分離した日本株専用の Next.js アプリ。

- 銘柄は証券コード4桁（例: 7203）で登録する
- 金額表示は円（¥ / Intl.NumberFormat ja-JP・JPY、`src/lib/format.ts`）
- API は日本株版バックエンドの `/api/jp/...` を呼ぶ
  （`/api/jp/watchlist`、`/api/jp/watchlist/{code}`、`/api/jp/jobs/daily-check` 等）
- データソース前提: J-Quants（株価・財務サマリー）/ EDINET（財務詳細）

## e2e テストについて

米国版の Playwright e2e（`e2e/`、`playwright.config.ts`、`test:e2e`）は
本アプリには持ち込んでいない。**CI 対象外**であり、検証は
`npm run build`（型チェック込み）で行う。

## 起動

```bash
npm run dev   # NEXT_PUBLIC_API_URL で日本株版バックエンドを指定
```
