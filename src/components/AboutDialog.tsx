import { useEffect } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

type Props = {
  open: boolean;
  version: string;
  onClose: () => void;
};

const SITE_URL = "https://legost.in";

/// Lightweight About modal: app name + version + a clickable link to the
/// author's site. Opens via the Tauri opener plugin so the user's default
/// browser handles `https://legost.in` instead of the webview navigating
/// away from the app.
export function AboutDialog({ open, version, onClose }: Props) {
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
          Локальная читалка и каталогизатор для библиотек в формате INPX
          (Флибуста). FB2/EPUB ридер, экспорт, OPDS-шеринг.
        </p>
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
        <div className="about-actions">
          <button onClick={onClose}>Закрыть</button>
        </div>
      </div>
    </div>
  );
}
