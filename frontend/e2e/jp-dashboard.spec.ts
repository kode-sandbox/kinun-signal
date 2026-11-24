import { test, expect } from "@playwright/test";

// Smoke tests for the Japanese-stock dashboard at /jp.
// As with the US dashboard spec, no backend runs during E2E, so the
// page falls back to its seeded FALLBACK data (7203 / 6758) and must
// still render fully. Modals close via their buttons, not Escape.

test("jp dashboard renders with fallback data and no uncaught errors", async ({
  page,
}) => {
  const pageErrors: string[] = [];
  page.on("pageerror", (e) => pageErrors.push(String(e)));

  await page.goto("/jp");

  // Header / logo for the Japanese market
  await expect(page.getByText("kinun-signal JP").first()).toBeVisible();

  // KPI bar
  await expect(page.getByText("監視中").first()).toBeVisible();

  // Stock cards come from the fallback watchlist when the API is unreachable
  await expect(page.getByText("7203").first()).toBeVisible({
    timeout: 15_000,
  });
  await expect(page.getByText("6758").first()).toBeVisible();

  expect(pageErrors).toEqual([]);
});

test("jp stock detail modal opens from a card and closes", async ({
  page,
}) => {
  await page.goto("/jp");
  await page.getByText("7203").first().click();

  const modal = page.locator("div.fixed.inset-0");
  await expect(modal.getByText("算出根拠").first()).toBeVisible({
    timeout: 10_000,
  });
  await expect(modal.getByText("算出履歴").first()).toBeVisible();

  await modal.getByRole("button", { name: "閉じる" }).click();
  await expect(modal).toBeHidden({ timeout: 5_000 });
});

test("jp add-stock modal opens and closes", async ({ page }) => {
  await page.goto("/jp");
  await page.getByRole("button", { name: /銘柄を追加/ }).first().click();

  const modal = page.locator("div.fixed.inset-0");
  await expect(modal.getByText("証券コード").first()).toBeVisible({
    timeout: 10_000,
  });

  await modal.getByRole("button", { name: "キャンセル" }).click();
  await expect(modal).toBeHidden({ timeout: 5_000 });
});

test("market switch via the hamburger side menu", async ({ page }) => {
  await page.goto("/");

  // Open the side menu and switch to the Japanese dashboard
  await page.getByRole("button", { name: "メニューを開く" }).click();
  await page.getByRole("link", { name: /日本株ダッシュボード/ }).click();
  await expect(page).toHaveURL(/\/jp$/);
  await expect(page.getByText("kinun-signal JP").first()).toBeVisible({
    timeout: 15_000,
  });
  await expect(page.getByText("7203").first()).toBeVisible();

  // Switch back to the US dashboard the same way
  await page.getByRole("button", { name: "メニューを開く" }).click();
  await page.getByRole("link", { name: /米国株ダッシュボード/ }).click();
  await expect(page).toHaveURL(/\/$/);
  await expect(
    page.getByText("Personal Stock Analyst").first()
  ).toBeVisible({ timeout: 15_000 });
});
