import type { ImportProgress } from "../lib/types";

type Props = { progress: ImportProgress };

const STAGE_LABEL: Record<string, string> = {
  reading: "Чтение INPX",
  indexing: "Индексация",
  done: "Готово",
};

export function ImportOverlay({ progress }: Props) {
  const total = progress.bytesTotal;
  const done = Math.min(progress.bytesDone, total);
  const pct = total > 0 ? Math.min(100, (done / total) * 100) : 0;
  const known = total > 0;
  const stage = STAGE_LABEL[progress.stage] ?? progress.stage;

  return (
    <div className="import-overlay" role="status" aria-live="polite">
      <div className="import-card">
        <div className="import-stage">{stage}</div>
        <progress
          className={known ? "" : "indeterminate"}
          value={known ? done : undefined}
          max={known ? total : undefined}
        />
        <div className="import-stats">
          <span>{progress.records.toLocaleString("ru")} записей</span>
          {known && (
            <span>
              {formatMB(done)} / {formatMB(total)} МБ · {pct.toFixed(1)}%
            </span>
          )}
        </div>
      </div>
    </div>
  );
}

function formatMB(bytes: number): string {
  return (bytes / (1024 * 1024)).toLocaleString("ru", {
    maximumFractionDigits: 1,
    minimumFractionDigits: 1,
  });
}
