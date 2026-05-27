import { useCallback, useEffect, useRef, useState } from "react";

const SIX_HOURS_MS = 6 * 60 * 60 * 1000;
const STARTUP_DELAY_MS = 5_000;

export type UpdateProgress = {
  downloaded: number;
  total: number;
};

export type UpdaterStatus =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "installing"
  | "error";

export type UpdateInfo = {
  version: string;
  currentVersion: string;
  date?: string;
  body?: string;
};

export type UpdaterState = {
  status: UpdaterStatus;
  update: UpdateInfo | null;
  progress: UpdateProgress | null;
  error: string | null;
  lastCheck: number | null;
};

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export function useUpdater() {
  const [state, setState] = useState<UpdaterState>({
    status: "idle",
    update: null,
    progress: null,
    error: null,
    lastCheck: null,
  });

  // Hold the live Update handle so the same instance is reused for install
  // after check() resolved it. Recreating it would re-download metadata and
  // double-charge the GitHub bandwidth quota.
  const handleRef = useRef<unknown>(null);

  const check = useCallback(async (opts?: { silent?: boolean }) => {
    if (!inTauri) return;
    if (!opts?.silent) {
      setState((s) => ({ ...s, status: "checking", error: null }));
    }
    try {
      const mod = await import("@tauri-apps/plugin-updater");
      const update = await mod.check();
      if (!update) {
        handleRef.current = null;
        setState((s) => ({
          ...s,
          status: "idle",
          update: null,
          progress: null,
          error: null,
          lastCheck: Date.now(),
        }));
        return;
      }
      handleRef.current = update;
      setState({
        status: "available",
        update: {
          version: update.version,
          currentVersion: update.currentVersion,
          date: update.date,
          body: update.body,
        },
        progress: null,
        error: null,
        lastCheck: Date.now(),
      });
    } catch (e) {
      const msg = String((e as Error)?.message ?? e);
      // Background sweeps shouldn't surface transient network errors as a
      // red banner — only the explicit "Проверить" button does.
      if (opts?.silent) {
        setState((s) => ({ ...s, lastCheck: Date.now() }));
        return;
      }
      setState((s) => ({ ...s, status: "error", error: msg }));
    }
  }, []);

  const install = useCallback(async () => {
    if (!inTauri) return;
    const handle = handleRef.current as {
      downloadAndInstall: (
        cb: (e: {
          event: "Started" | "Progress" | "Finished";
          data?: { contentLength?: number; chunkLength?: number };
        }) => void,
      ) => Promise<void>;
    } | null;
    if (!handle) {
      setState((s) => ({
        ...s,
        status: "error",
        error: "Сначала нужно проверить обновления",
      }));
      return;
    }
    setState((s) => ({
      ...s,
      status: "downloading",
      progress: { downloaded: 0, total: 0 },
      error: null,
    }));
    try {
      let total = 0;
      let downloaded = 0;
      await handle.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data?.contentLength ?? 0;
          setState((s) => ({
            ...s,
            status: "downloading",
            progress: { downloaded: 0, total },
          }));
        } else if (event.event === "Progress") {
          downloaded += event.data?.chunkLength ?? 0;
          setState((s) => ({
            ...s,
            status: "downloading",
            progress: { downloaded, total },
          }));
        } else if (event.event === "Finished") {
          setState((s) => ({
            ...s,
            status: "installing",
            progress: { downloaded: total || downloaded, total },
          }));
        }
      });
      const proc = await import("@tauri-apps/plugin-process");
      await proc.relaunch();
    } catch (e) {
      setState((s) => ({
        ...s,
        status: "error",
        error: String((e as Error)?.message ?? e),
      }));
    }
  }, []);

  // Boot-time check (after a brief delay so the app paints) + 6h interval
  // while the window stays open.
  useEffect(() => {
    if (!inTauri) return;
    const startup = window.setTimeout(() => void check({ silent: true }), STARTUP_DELAY_MS);
    const interval = window.setInterval(() => void check({ silent: true }), SIX_HOURS_MS);
    return () => {
      window.clearTimeout(startup);
      window.clearInterval(interval);
    };
  }, [check]);

  return { state, check, install };
}
