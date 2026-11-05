import { test, expect } from "@playwright/test";

// Smoke tests for the dashboard.
// No backend runs during E2E, so the app must fall back to its seeded
// FALLBACK data (AAPL / NVDA) and still render the full dashboard.
// The app's modals do not close on Escape (see the components); they
// close via their キャンセル / 閉じる buttons, so the tests do the same.

test("dashboard renders with fallback data and no uncaught errors", async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (e) => pageErrors.push(String(e)));

  await page.goto("/");

  // Header / logo
  await expect(page.getByText("Personal Stock Analyst").first()).toBeVisible();

  // KPI bar
  await expect(page.getByText("監視中").first()).toBeVisible();

  // Stock cards come from the fallback watchlist when the API is unreachable
  await expect(page.getByText("AAPL").first()).toBeVisible({
    timeout: 15_000,
  });
  await expect(page.getByText("NVDA").first()).toBeVisible();

  expect(pageErrors).toEqual([]);
});

test("stock detail modal opens from a card and closes", async ({ page }) => {
  await page.goto("/");
  await page.getByText("AAPL").first().click();

  const modal = page.locator("div.fixed.inset-0");
  await expect(modal.getByText("算出根拠").first()).toBeVisible({
    timeout: 10_000,
  });
  await expect(modal.getByText("算出履歴").first()).toBeVisible();

  await modal.getByRole("button", { name: "閉じる" }).click();
  await expect(modal).toBeHidden({ timeout: 5_000 });
});

test("add-stock modal opens and closes", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: /銘柄を追加/ }).first().click();

  const modal = page.locator("div.fixed.inset-0");
  await expect(modal.getByText("ティッカーシンボル").first()).toBeVisible({
    timeout: 10_000,
  });

  await modal.getByRole("button", { name: "キャンセル" }).click();
  await expect(modal).toBeHidden({ timeout: 5_000 });
});
