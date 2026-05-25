import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/api";
import { genreLabel } from "../lib/genresRu";
import type {
  AuthorHit,
  BookFilters,
  GenreHit,
  LanguageHit,
  SearchScope,
} from "../lib/types";

type Props = {
  open: boolean;
  onClose: () => void;
  scope: SearchScope;
  onScopeChange: (s: SearchScope) => void;
  filters: BookFilters;
  onChange: (next: BookFilters) => void;
  languages: LanguageHit[];
  /// Pre-resolved author display name for the currently selected `authorId`,
  /// so we can render its label without re-fetching every render.
  currentAuthorLabel?: string | null;
};

/// Popover-style filter sheet: scope (for search), language, genre, author
/// (with type-ahead), and the archive (physical pack) filter.
export function FilterPanel({
  open,
  onClose,
  scope,
  onScopeChange,
  filters,
  onChange,
  languages,
  currentAuthorLabel,
}: Props) {
  const [genres, setGenres] = useState<GenreHit[]>([]);
  const [genreFilter, setGenreFilter] = useState("");

  const [authorQuery, setAuthorQuery] = useState("");
  const [authorSuggestions, setAuthorSuggestions] = useState<AuthorHit[]>([]);

  const panelRef = useRef<HTMLDivElement | null>(null);

  // Load genre list once when the panel opens for the first time.
  useEffect(() => {
    if (!open) return;
    if (genres.length === 0) {
      api.genres().then(setGenres).catch(() => {});
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  // Type-ahead author search. We use the FTS-backed search so partial words
  // like "тол" match "Толстой". Empty query clears the suggestions.
  useEffect(() => {
    if (!open) return;
    const q = authorQuery.trim();
    if (q.length < 2) {
      setAuthorSuggestions([]);
      return;
    }
    let cancelled = false;
    const t = window.setTimeout(() => {
      api
        .search(q, "authors", { ...filters, authorId: null }, 12)
        .then((r) => {
          if (cancelled) return;
          setAuthorSuggestions(r.authors);
        })
        .catch(() => {
          /* ignore */
        });
    }, 180);
    return () => {
      cancelled = true;
      window.clearTimeout(t);
    };
    // We intentionally don't depend on `filters.authorId` — picking one
    // shouldn't re-run the query.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [authorQuery, open, filters.lang, filters.genre, filters.archive]);

  // Close on outside click.
  useEffect(() => {
    if (!open) return;
    const onDoc = (e: MouseEvent) => {
      if (!panelRef.current) return;
      if (!panelRef.current.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    // Defer attaching for one tick so the opening click doesn't immediately close.
    const t = window.setTimeout(() => document.addEventListener("mousedown", onDoc), 0);
    document.addEventListener("keydown", onKey);
    return () => {
      window.clearTimeout(t);
      document.removeEventListener("mousedown", onDoc);
      document.removeEventListener("keydown", onKey);
    };
  }, [open, onClose]);

  const filteredGenres = useMemo(() => {
    const q = genreFilter.trim().toLowerCase();
    if (!q) return genres.slice(0, 60);
    return genres
      .filter(
        (g) =>
          g.code.toLowerCase().includes(q) ||
          genreLabel(g.code).toLowerCase().includes(q),
      )
      .slice(0, 60);
  }, [genres, genreFilter]);

  if (!open) return null;

  const set = <K extends keyof BookFilters>(key: K, value: BookFilters[K]) =>
    onChange({ ...filters, [key]: value });

  return (
    <div className="filter-panel" ref={panelRef}>
      <div className="filter-section">
        <label className="filter-label">Что искать</label>
        <div className="filter-chips">
          {(["all", "authors", "series", "books"] as SearchScope[]).map((s) => (
            <button
              key={s}
              className={`chip${scope === s ? " active" : ""}`}
              onClick={() => onScopeChange(s)}
            >
              {s === "all" && "Всё"}
              {s === "authors" && "Авторы"}
              {s === "series" && "Серии"}
              {s === "books" && "Книги"}
            </button>
          ))}
        </div>
      </div>

      <div className="filter-section">
        <label className="filter-label">Язык</label>
        <select
          className="filter-select"
          value={filters.lang ?? ""}
          onChange={(e) => set("lang", e.target.value || null)}
        >
          <option value="">— любой —</option>
          {languages.map((l) => (
            <option key={l.code} value={l.code}>
              {l.code} ({l.count.toLocaleString("ru")})
            </option>
          ))}
        </select>
      </div>

      <div className="filter-section">
        <label className="filter-label">
          Жанр
          {filters.genre && (
            <button
              className="filter-clear"
              onClick={() => set("genre", null)}
              title="Сбросить"
            >
              ×
            </button>
          )}
        </label>
        {filters.genre ? (
          <div className="filter-current" title={filters.genre}>
            {genreLabel(filters.genre)}
          </div>
        ) : (
          <>
            <input
              type="search"
              className="filter-input"
              placeholder="Поиск по жанрам…"
              value={genreFilter}
              onChange={(e) => setGenreFilter(e.target.value)}
            />
            <div className="filter-options">
              {filteredGenres.map((g) => (
                <button
                  key={g.code}
                  className="filter-option"
                  onClick={() => set("genre", g.code)}
                  title={`${g.code} · ${g.count.toLocaleString("ru")} книг`}
                >
                  <span className="filter-option-label">
                    {genreLabel(g.code)}
                  </span>
                  <span className="filter-option-count">
                    {g.count.toLocaleString("ru")}
                  </span>
                </button>
              ))}
              {filteredGenres.length === 0 && (
                <div className="muted small">Ничего не нашлось</div>
              )}
            </div>
          </>
        )}
      </div>

      <div className="filter-section">
        <label className="filter-label">
          Автор
          {filters.authorId != null && (
            <button
              className="filter-clear"
              onClick={() => set("authorId", null)}
              title="Сбросить"
            >
              ×
            </button>
          )}
        </label>
        {filters.authorId != null ? (
          <div className="filter-current">
            {currentAuthorLabel ?? `id ${filters.authorId}`}
          </div>
        ) : (
          <>
            <input
              type="search"
              className="filter-input"
              placeholder="Введите часть имени…"
              value={authorQuery}
              onChange={(e) => setAuthorQuery(e.target.value)}
            />
            <div className="filter-options">
              {authorSuggestions.map((a) => (
                <button
                  key={a.id}
                  className="filter-option"
                  onClick={() => {
                    set("authorId", a.id);
                    setAuthorQuery("");
                    setAuthorSuggestions([]);
                  }}
                >
                  <span className="filter-option-label">{a.display}</span>
                  <span className="filter-option-count">{a.bookCount}</span>
                </button>
              ))}
              {authorQuery.trim().length >= 2 &&
                authorSuggestions.length === 0 && (
                  <div className="muted small">Никого не нашлось</div>
                )}
              {authorQuery.trim().length < 2 && (
                <div className="muted small">Минимум 2 символа</div>
              )}
            </div>
          </>
        )}
      </div>

      <div className="filter-footer">
        <button
          className="link"
          onClick={() =>
            onChange({ lang: null, genre: null, archive: null, authorId: null })
          }
        >
          Сбросить всё
        </button>
        <button className="primary" onClick={onClose}>
          Готово
        </button>
      </div>
    </div>
  );
}

/// Count of non-default filter fields, used for the badge.
export function countActiveFilters(f: BookFilters): number {
  let n = 0;
  if (f.lang) n++;
  if (f.genre) n++;
  if (f.archive) n++;
  if (f.authorId != null) n++;
  return n;
}
