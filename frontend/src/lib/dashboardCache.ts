import { AlertLog, WatchlistItem } from "@/types/stock";

/**
 * Browser-side cache of the dashboard's initial data (watchlist, alert
 * history, LLM model name), used to render the previous data instantly on
 * load while fresh data is fetched in the background
 * (stale-while-revalidate). Only data from a successful backend fetch is
 * ever stored here — mock/fallback data must never be persisted.
 */
export interface DashboardCache {
  savedAt: number;
  watchlist: WatchlistItem[];
  alerts: AlertLog[];
  llmModel: string | null;
}

const CACHE_KEY = "psa-dashboard-cache-v1";

/**
 * Cache key for the Japanese-stock dashboard. The US dashboard keeps the
 * original key so existing cached data stays valid; each market must use
 * its own key because both dashboards share the same origin.
 */
export const JP_DASHBOARD_CACHE_KEY = "psa-dashboard-cache-v1-jp";
const CACHE_VERSION = 1;
const MAX_CACHE_AGE_MS = 7 * 24 * 60 * 60 * 1000; // discard cache older than 7 days

interface StoredDashboardCache extends DashboardCache {
  version: number;
}

export function loadDashboardCache(
  cacheKey: string = CACHE_KEY
): DashboardCache | null {
  try {
    const raw = localStorage.getItem(cacheKey);
    if (!raw) {
      return null;
    }
    const parsed = JSON.parse(raw) as Partial<StoredDashboardCache>;
    if (parsed.version !== CACHE_VERSION) {
      return null;
    }
    if (typeof parsed.savedAt !== "number") {
      return null;
    }
    if (Date.now() - parsed.savedAt > MAX_CACHE_AGE_MS) {
      return null;
    }
    if (!Array.isArray(parsed.watchlist) || !Array.isArray(parsed.alerts)) {
      return null;
    }
    return {
      savedAt: parsed.savedAt,
      watchlist: parsed.watchlist,
      alerts: parsed.alerts,
      llmModel: typeof parsed.llmModel === "string" ? parsed.llmModel : null,
    };
  } catch {
    // Corrupted entry or storage unavailable (private mode etc.)
    return null;
  }
}

export function saveDashboardCache(
  data: Omit<DashboardCache, "savedAt">,
  cacheKey: string = CACHE_KEY
): void {
  try {
    const stored: StoredDashboardCache = {
      version: CACHE_VERSION,
      savedAt: Date.now(),
      watchlist: data.watchlist,
      alerts: data.alerts,
      llmModel: data.llmModel,
    };
    localStorage.setItem(cacheKey, JSON.stringify(stored));
  } catch {
    // Best-effort only: quota exceeded or storage unavailable.
  }
}

/**
 * Removes a ticker from the cached watchlist in place, preserving the
 * original savedAt, so a deleted stock does not briefly reappear from the
 * cache on the next visit.
 */
export function removeTickerFromDashboardCache(
  ticker: string,
  cacheKey: string = CACHE_KEY
): void {
  try {
    const raw = localStorage.getItem(cacheKey);
    if (!raw) {
      return;
    }
    const parsed = JSON.parse(raw) as Partial<StoredDashboardCache>;
    if (parsed.version !== CACHE_VERSION || !Array.isArray(parsed.watchlist)) {
      return;
    }
    parsed.watchlist = parsed.watchlist.filter((item) => item.ticker !== ticker);
    localStorage.setItem(cacheKey, JSON.stringify(parsed));
  } catch {
    // Best-effort only.
  }
}
