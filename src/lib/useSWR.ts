import { useEffect, useRef, useState } from "react";
import { cacheKey, fetchAndCache, getCached } from "./cache";

type State<T> = {
  data: T | null;
  loading: boolean;
  error: string | null;
  /// True when `data` came from cache and may be stale (a fresh fetch is in
  /// flight). Lets the UI show a subtle "refreshing" hint without blanking
  /// the list.
  stale: boolean;
};

/// Stale-while-revalidate hook tuned for our list endpoints.
///
/// - `endpoint`: arbitrary cache namespace (e.g. `"list_genres"`).
/// - `params`: serialized into the cache key; pass the same object shape as
///   the underlying API call.
/// - `load`: async fetcher executed when there's no fresh cached value.
/// - `enabled`: if `false`, the hook stays idle (useful when a prerequisite
///   like `book.lib_id` isn't known yet).
///
/// On dependency change we render the previous result synchronously from the
/// cache and kick off a background refetch; the resulting state's `stale`
/// flag flips to `false` once new data lands.
export function useSWR<T>(
  endpoint: string,
  params: unknown,
  load: () => Promise<T>,
  enabled = true,
): State<T> {
  const key = cacheKey(endpoint, params);
  const [state, setState] = useState<State<T>>(() => {
    if (!enabled) return { data: null, loading: false, error: null, stale: false };
    const cached = getCached<T>(key);
    return {
      data: cached,
      loading: cached == null,
      error: null,
      stale: cached != null,
    };
  });

  // Latch the latest load function so re-renders don't double-fire.
  const loadRef = useRef(load);
  loadRef.current = load;

  useEffect(() => {
    if (!enabled) {
      setState({ data: null, loading: false, error: null, stale: false });
      return;
    }
    let cancelled = false;
    const cached = getCached<T>(key);
    setState({
      data: cached,
      loading: cached == null,
      error: null,
      stale: cached != null,
    });
    fetchAndCache<T>(key, loadRef.current)
      .then((data) => {
        if (cancelled) return;
        setState({ data, loading: false, error: null, stale: false });
      })
      .catch((e) => {
        if (cancelled) return;
        setState((prev) => ({
          data: prev.data,
          loading: false,
          error: String(e),
          stale: prev.stale,
        }));
      });
    return () => {
      cancelled = true;
    };
  }, [key, enabled]);

  return state;
}
