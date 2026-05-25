import { useCallback, useEffect, useRef, useState } from "react";
import { api, onShareStatus } from "../lib/api";
import type { ShareStatus } from "../lib/types";

type Props = {
  disabled?: boolean;
};

const AUTO_DOMAIN = "__auto__";

export function ShareBar({ disabled }: Props) {
  const [status, setStatus] = useState<ShareStatus>({
    running: false,
    localUrl: null,
    publicUrl: null,
    error: null,
    stage: "stopped",
  });
  const [busy, setBusy] = useState(false);
  const [copied, setCopied] = useState(false);
  const [domains, setDomains] = useState<string[]>([]);
  const [domain, setDomain] = useState<string>(AUTO_DOMAIN);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    void api.shareStatus().then((s) => {
      if (!cancelled) setStatus(s);
    });
    void api
      .shareListDomains()
      .then((list) => {
        if (cancelled) return;
        setDomains(list);
      })
      .catch(() => {
        // domain discovery is best-effort
      });
    void onShareStatus((s) => setStatus(s)).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
      if (copyTimer.current) clearTimeout(copyTimer.current);
    };
  }, []);

  const startWith = useCallback(
    async (pooling: boolean) => {
      setBusy(true);
      try {
        const chosen = domain === AUTO_DOMAIN ? null : domain;
        const s = await api.shareStart({ domain: chosen, pooling });
        setStatus(s);
      } catch (e) {
        setStatus((cur) => ({
          ...cur,
          running: false,
          stage: "error",
          error: String(e),
        }));
      } finally {
        setBusy(false);
      }
    },
    [domain],
  );

  const onToggle = useCallback(async () => {
    if (status.running) {
      setBusy(true);
      try {
        const s = await api.shareStop();
        setStatus(s);
      } catch (e) {
        setStatus((cur) => ({
          ...cur,
          stage: "error",
          error: String(e),
        }));
      } finally {
        setBusy(false);
      }
    } else {
      await startWith(false);
    }
  }, [status.running, startWith]);

  const onCopy = useCallback(async () => {
    if (!status.publicUrl) return;
    try {
      await navigator.clipboard.writeText(`${status.publicUrl}/opds`);
      setCopied(true);
      if (copyTimer.current) clearTimeout(copyTimer.current);
      copyTimer.current = setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard may be unavailable; ignore silently.
    }
  }, [status.publicUrl]);

  const onKillStray = useCallback(async () => {
    setBusy(true);
    try {
      const killed = await api.shareKillStray();
      setStatus((cur) => ({
        ...cur,
        error:
          killed > 0
            ? `Очищено: ${killed}. Пробую переподключиться…`
            : "Не нашлось ни локальных процессов, ни сессий ngrok API. Возможно, нет API-ключа — попробуйте «С pooling» или подождите ~30с пока edge сам освободит endpoint.",
        stage: "stopped",
      }));
      if (killed > 0) {
        // Give the edge a couple seconds to settle, then retry.
        await new Promise((r) => setTimeout(r, 1500));
        await startWith(false);
      }
    } catch (e) {
      setStatus((cur) => ({ ...cur, error: String(e), stage: "error" }));
    } finally {
      setBusy(false);
    }
  }, [startWith]);

  const onRetryPooling = useCallback(async () => {
    await startWith(true);
  }, [startWith]);

  const isStarting = status.stage === "starting" || busy;
  const label = status.running
    ? "Остановить каталог"
    : isStarting
      ? "Запуск…"
      : "Поднять каталог OPDS";

  const klass = ["share-toggle", status.running ? "on" : "", status.stage === "error" ? "err" : ""]
    .filter(Boolean)
    .join(" ");

  // Detect the specific orphan-tunnel error so we can offer one-click recovery.
  const isStrayError =
    !!status.error &&
    (status.error.includes("ERR_NGROK_334") ||
      status.error.toLowerCase().includes("already online"));

  return (
    <div className="share-bar">
      <button
        type="button"
        className={klass}
        onClick={onToggle}
        disabled={disabled || isStarting}
        title={
          status.running
            ? "Открытый OPDS-каталог. Нажмите, чтобы остановить."
            : "Запустить локальный OPDS-сервер и проброс через ngrok"
        }
      >
        <span className="share-dot" />
        {label}
      </button>
      {!status.running && domains.length > 0 && (
        <select
          className="share-domain"
          value={domain}
          onChange={(e) => setDomain(e.target.value)}
          disabled={disabled || isStarting}
          title="Какой публичный домен использовать"
        >
          <option value={AUTO_DOMAIN}>Авто (любой свободный)</option>
          {domains.map((d) => (
            <option key={d} value={d}>
              {d}
            </option>
          ))}
        </select>
      )}
      {status.running && status.publicUrl && (
        <div className="share-url">
          <a
            href={`${status.publicUrl}/opds`}
            target="_blank"
            rel="noreferrer"
            title="Открыть OPDS в браузере"
          >
            {trim(status.publicUrl)}/opds
          </a>
          <button
            type="button"
            className="icon-btn"
            onClick={onCopy}
            title="Скопировать публичную ссылку на OPDS"
          >
            {copied ? "✓" : "⧉"}
          </button>
        </div>
      )}
      {!status.running && status.localUrl && status.stage !== "error" && (
        <div className="share-url muted">{status.localUrl}</div>
      )}
      {status.error && (
        <div className="share-error" title={status.error}>
          {status.error}
        </div>
      )}
      {!status.running && isStrayError && (
        <div className="share-recovery">
          <button
            type="button"
            className="share-kill"
            onClick={onKillStray}
            disabled={isStarting}
            title="Локальный pkill + ngrok api tunnel-sessions stop / endpoints delete (нужен API-ключ)"
          >
            Очистить через API
          </button>
          <button
            type="button"
            className="share-kill"
            onClick={onRetryPooling}
            disabled={isStarting}
            title="Перезапустить ngrok с флагом --pooling-enabled — встанет поверх существующего endpoint"
          >
            Запустить с pooling
          </button>
        </div>
      )}
    </div>
  );
}

function trim(url: string): string {
  return url.replace(/^https?:\/\//, "");
}
