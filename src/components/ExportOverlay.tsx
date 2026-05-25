import type { ExportProgress, ExportSummary } from "../lib/types";

type Props = {
  progress: ExportProgress | null;
  summary: ExportSummary | null;
  onDismiss: () => void;
};

const STAGE_LABEL: Record<string, string> = {
  starting: "Готовлю",
  copying: "Копирую",
  done: "Готово",
};

export function ExportOverlay({ progress, summary, onDismiss }: Props) {
  if (!progress && !summary) return null;

  return (
    <div className="export-overlay" role="status" aria-live="polite">
      <div className="export-card">
        {summary ? (
          <Summary summary={summary} onDismiss={onDismiss} />
        ) : progress ? (
          <Progress p={progress} />
        ) : null}
      </div>
    </div>
  );
}

function Progress({ p }: { p: ExportProgress }) {
  const total = p.total;
  const done = Math.min(p.done, total);
  const pct = total > 0 ? Math.min(100, (done / total) * 100) : 0;
  const stage = STAGE_LABEL[p.stage] ?? p.stage;
  return (
    <>
      <div className="export-stage">{stage}</div>
      <progress value={done} max={total} />
      <div className="export-stats">
        <span>
          {done} / {total} книг
        </span>
        {pct > 0 && <span>{pct.toFixed(1)}%</span>}
      </div>
      {p.current && <div className="export-current">{p.current}</div>}
    </>
  );
}

function Summary({
  summary,
  onDismiss,
}: {
  summary: ExportSummary;
  onDismiss: () => void;
}) {
  return (
    <>
      <div className="export-stage">Экспорт завершён</div>
      <div className="export-summary-stats">
        <div>
          <span className="muted">Скопировано</span>
          <strong>{summary.copied}</strong>
        </div>
        <div>
          <span className="muted">Пропущено</span>
          <strong>{summary.skipped}</strong>
        </div>
        <div>
          <span className="muted">Всего</span>
          <strong>{summary.total}</strong>
        </div>
      </div>
      <div className="export-target" title={summary.targetDir}>
        <code>{shorten(summary.targetDir)}</code>
      </div>
      {summary.errors.length > 0 && (
        <details className="export-errors">
          <summary>Ошибки: {summary.errors.length}</summary>
          <ul>
            {summary.errors.slice(0, 20).map((e, i) => (
              <li key={i}>
                <strong>{e.title || `#${e.bookId}`}</strong> — {e.message}
              </li>
            ))}
            {summary.errors.length > 20 && (
              <li className="muted">… и ещё {summary.errors.length - 20}</li>
            )}
          </ul>
        </details>
      )}
      <div className="export-actions">
        <button className="primary" onClick={onDismiss}>
          Закрыть
        </button>
      </div>
    </>
  );
}

function shorten(p: string): string {
  if (p.length <= 60) return p;
  return "…" + p.slice(-59);
}
