import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { BookFilters, SeriesHit } from "../lib/types";
import { useSWR } from "../lib/useSWR";

type Props = {
  filters: BookFilters;
  onPickSeries: (name: string) => void;
};

/// Alphabet index for series — letter → all series starting with that letter.
/// Single-letter buckets (no two-letter sub-prefixes) because series counts
/// per letter are much lower than authors.
export function SeriesAlphabet({ filters, onPickSeries }: Props) {
  const [picked, setPicked] = useState<string | null>(null);
  const [textFilter, setTextFilter] = useState("");

  const lettersQ = useSWR<[string, number][]>(
    "list_series_letters",
    { filters },
    () => api.seriesLetters(filters),
  );
  const letters = lettersQ.data ? sortLettersCyrillicFirst(lettersQ.data) : [];

  useEffect(() => {
    if (picked && lettersQ.data && !lettersQ.data.some(([l]) => l === picked)) {
      setPicked(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lettersQ.data]);

  const itemsQ = useSWR<SeriesHit[]>(
    "list_series_by_letter",
    { letter: picked, filters },
    () => api.seriesByLetter(picked!, filters),
    picked != null,
  );
  const items = itemsQ.data ?? [];

  useEffect(() => {
    setTextFilter("");
  }, [picked]);

  const loading = lettersQ.loading || itemsQ.loading;
  const refreshing = lettersQ.stale || itemsQ.stale;
  const error = lettersQ.error || itemsQ.error;

  const filtered = textFilter.trim()
    ? items.filter((s) =>
        s.name.toLowerCase().includes(textFilter.trim().toLowerCase()),
      )
    : items;

  return (
    <div className="entity-page alphabet-index">
      <header className="entity-hero">
        <div className="section-label">Каталог</div>
        <h1 className="entity-title">Серии</h1>
      </header>

      <div className="alphabet-trail">
        <button
          className={`alphabet-crumb${!picked ? " active" : ""}`}
          onClick={() => setPicked(null)}
        >
          А–Я
        </button>
        {picked && (
          <>
            <span className="alphabet-trail-sep">›</span>
            <span className="alphabet-crumb active">{picked}</span>
          </>
        )}
      </div>

      {!picked && (
        <div className={`alphabet-letters${refreshing ? " refreshing" : ""}`}>
          {loading && letters.length === 0 && (
            <span className="muted small">Загружаю…</span>
          )}
          {!loading && letters.length === 0 && (
            <span className="muted small">Нет серий под этими фильтрами</span>
          )}
          {letters.map(([lt, cnt]) => (
            <button
              key={lt}
              className="alphabet-letter"
              onClick={() => setPicked(lt)}
              title={`${cnt.toLocaleString("ru")} серий`}
            >
              <span className="alphabet-letter-char">{lt}</span>
              <span className="alphabet-letter-count">
                {cnt.toLocaleString("ru")}
              </span>
            </button>
          ))}
        </div>
      )}

      {picked && (
        <div className="alphabet-authors">
          <div className="alphabet-authors-head">
            <h3>Серии на «{picked}»</h3>
            <input
              type="search"
              className="alphabet-authors-filter"
              placeholder="Фильтр по названию…"
              value={textFilter}
              onChange={(e) => setTextFilter(e.target.value)}
            />
          </div>
          {loading && items.length === 0 && (
            <div className="muted small">Загружаю…</div>
          )}
          {!loading && filtered.length === 0 && (
            <div className="muted small">
              {items.length === 0 ? "Нет серий" : "Не подошло под фильтр"}
            </div>
          )}
          <ul className="row-list">
            {filtered.map((s) => (
              <li
                key={s.name}
                className="row series"
                onClick={() => onPickSeries(s.name)}
              >
                <div className="row-body">
                  <div className="row-title">{s.name}</div>
                </div>
                <span className="row-meta">{s.bookCount}</span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {error && <div className="content-error">{error}</div>}
    </div>
  );
}

function sortLettersCyrillicFirst(
  rows: [string, number][],
): [string, number][] {
  const bucket = (ch: string): number => {
    const code = ch.charCodeAt(0);
    if (code >= 0x0400 && code <= 0x04ff) return 0;
    if ((code >= 0x41 && code <= 0x5a) || (code >= 0x61 && code <= 0x7a))
      return 1;
    return 2;
  };
  return [...rows].sort((a, b) => {
    const ba = bucket(a[0]);
    const bb = bucket(b[0]);
    if (ba !== bb) return ba - bb;
    return a[0].localeCompare(b[0], "ru");
  });
}
