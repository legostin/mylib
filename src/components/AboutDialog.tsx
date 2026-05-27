import { useEffect } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { UpdaterState } from "../lib/updater";

type Props = {
  open: boolean;
  version: string;
  onClose: () => void;
  updater: UpdaterState;
  onCheck: () => void;
  onInstall: () => void;
};

const SITE_URL = "https://legost.in";

/// Lightweight About modal: app name + version + a clickable link to the
/// author's site. Opens via the Tauri opener plugin so the user's default
/// browser handles `https://legost.in` instead of the webview navigating
/// away from the app.
export function AboutDialog({
  open,
  version,
  onClose,
  updater,
  onCheck,
  onInstall,
}: Props) {
  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="about-backdrop" onMouseDown={onClose}>
      <div className="about-card" onMouseDown={(e) => e.stopPropagation()}>
        <div className="about-head">MyLib</div>
        <div className="about-version">версия {version}</div>
        <p className="about-text">
          Локальная читалка и каталогизатор для библиотек в формате INPX.
          FB2/EPUB ридер, экспорт, OPDS-шеринг.
        </p>
        <div className="about-author">Автор: Легостин Вячеслав</div>
        <div className="about-link-row">
          <button
            className="link about-link"
            onClick={() => {
              void openUrl(SITE_URL).catch(() => {
                // Fallback when the opener plugin isn't available (e.g.
                // running in a plain Vite dev outside Tauri).
                window.open(SITE_URL, "_blank");
              });
            }}
            title="Открыть в браузере"
          >
            {SITE_URL}
          </button>
        </div>
        <UpdaterSection
          version={version}
          state={updater}
          onCheck={onCheck}
          onInstall={onInstall}
        />
        <div className="about-actions">
          <button onClick={onClose}>Закрыть</button>
        </div>
      </div>
    </div>
  );
}

function UpdaterSection({
  version,
  state,
  onCheck,
  onInstall,
}: {
  version: string;
  state: UpdaterState;
  onCheck: () => void;
  onInstall: () => void;
}) {
  const busy = state.status === "checking" || state.status === "downloading" || state.status === "installing";

  let line: string | null = null;
  if (state.status === "checking") line = "Проверяю обновления…";
  else if (state.status === "downloading") {
    const { downloaded = 0, total = 0 } = state.progress ?? {};
    const pct = total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : null;
    line = pct != null ? `Загрузка ${pct}%…` : "Загрузка…";
  } else if (state.status === "installing") line = "Устанавливаю и перезапускаю…";
  else if (state.status === "available" && state.update)
    line = `Доступно обновление до ${state.update.version}.`;
  else if (state.status === "error" && state.error) line = `Ошибка: ${state.error}`;
  else if (state.lastCheck) line = `У вас установлена последняя версия (${version}).`;

  return (
    <div className="about-updater">
      {line && <div className="about-updater-line">{line}</div>}
      <div className="about-updater-actions">
        {state.status === "available" ? (
          <button className="primary" onClick={onInstall} disabled={busy}>
            Обновить до {state.update?.version}
          </button>
        ) : (
          <button onClick={onCheck} disabled={busy}>
            Проверить обновления
          </button>
        )}
      </div>
    </div>
  );
}
