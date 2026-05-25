/// Frontend cache for catalog list endpoints. Counts (genres, languages,
/// alphabet buckets, prefix breakdowns) recompute against the full books table
/// on each filter change, which is the slowest path in normal use; caching
/// the result by (endpoint, params) lets repeated navigation through the same
/// filter set feel instant.
///
/// Stale-while-revalidate pattern: callers can opt-in to receive the cached
/// (possibly stale) value immediately *and* a promise of the fresh value.
/// On import completion the whole cache is wiped (`invalidateAll`) since the
/// underlying SQLite rows have changed.

type Entry<T> = {
  data: T;
  ts: number;
};

const STORE = new Map<string, Entry<unknown>>();
const TTL_MS = 10 * 60_000; // 10 minutes — generous for a session of browsing
const subscribers: Set<() => void> = new Set();

export function cacheKey(endpoint: string, params: unknown): string {
  // Stable serialization: sort object keys at every level so `{a, b}` and
  // `{b, a}` hash the same. Plain JSON.stringify isn't deterministic for
  // object key order across all engines.
  return endpoint + ":" + stableStringify(params);
}

function stableStringify(v: unknown): string {
  if (v === null || typeof v !== "object") return JSON.stringify(v);
  if (Array.isArray(v)) {
    return "[" + v.map(stableStringify).join(",") + "]";
  }
  const obj = v as Record<string, unknown>;
  const keys = Object.keys(obj).sort();
  return (
    "{" +
    keys
      .map((k) => JSON.stringify(k) + ":" + stableStringify(obj[k]))
      .join(",") +
    "}"
  );
}

export function getCached<T>(key: string): T | null {
  const e = STORE.get(key);
  if (!e) return null;
  if (Date.now() - e.ts > TTL_MS) {
    STORE.delete(key);
    return null;
  }
  return e.data as T;
}

export function setCached<T>(key: string, value: T): void {
  STORE.set(key, { data: value, ts: Date.now() });
  for (const fn of subscribers) fn();
}

export function invalidateAll(): void {
  STORE.clear();
  for (const fn of subscribers) fn();
}

/// Cached invoke wrapper. Always fires a fresh request and updates the cache
/// on success; returns whatever the network returns. Use with `getCached`
/// when you need the stale value synchronously (e.g. for first render).
export async function fetchAndCache<T>(
  key: string,
  load: () => Promise<T>,
): Promise<T> {
  const result = await load();
  setCached(key, result);
  return result;
}

/// Subscribe to cache mutations. Used by the dev panel / debug UIs;
/// regular browse views don't need this.
export function onCacheChange(cb: () => void): () => void {
  subscribers.add(cb);
  return () => subscribers.delete(cb);
}
