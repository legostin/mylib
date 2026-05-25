import { useMemo, useState } from "react";
import { api } from "../lib/api";
import type { BookFilters, GenreHit } from "../lib/types";
import {
  CATEGORIES_RU,
  categoryFor,
  categoryLabel,
  genreLabel,
} from "../lib/genresRu";
import { useSWR } from "../lib/useSWR";

type Props = {
  filters: BookFilters;
  onPickGenre: (code: string) => void;
};

/// Genre catalogue: groups codes by FB2 category root and renders each in
/// Russian (with the raw code as a tooltip for power users).
export function GenresBrowse({ filters, onPickGenre }: Props) {
  const [textFilter, setTextFilter] = useState("");
  const { data, loading, stale, error } = useSWR<GenreHit[]>(
    "list_genres",
    { filters },
    () => api.genres(filters),
  );
  const genres = data ?? [];

  // Bucket genres into FB2 categories, ordered the way they appear in the
  // CATEGORIES_RU dictionary so users see "Фантастика" / "Проза" first.
  const grouped = useMemo(() => {
    const q = textFilter.trim().toLowerCase();
    const visible = q
      ? genres.filter(
          (g) =>
            g.code.toLowerCase().includes(q) ||
            genreLabel(g.code).toLowerCase().includes(q),
        )
      : genres;
    const buckets = new Map<string, GenreHit[]>();
    for (const g of visible) {
      const cat = categoryFor(g.code);
      if (!buckets.has(cat)) buckets.set(cat, []);
      buckets.get(cat)!.push(g);
    }
    // Sort within each category by ru label.
    for (const arr of buckets.values()) {
      arr.sort((a, b) =>
        genreLabel(a.code).localeCompare(genreLabel(b.code), "ru"),
      );
    }
    const order = Object.keys(CATEGORIES_RU);
    const out: [string, GenreHit[]][] = [];
    for (const cat of order) {
      if (buckets.has(cat)) {
        out.push([cat, buckets.get(cat)!]);
        buckets.delete(cat);
      }
    }
    // Any leftover buckets (shouldn't happen — categoryFor falls back to
    // "other") get appended alphabetically.
    for (const [cat, arr] of buckets) {
      out.push([cat, arr]);
    }
    return out;
  }, [genres, textFilter]);

  const total = genres.reduce((n, g) => n + g.count, 0);

  return (
    <div className="entity-page genres-browse">
      <header className="entity-hero">
        <div className="section-label">Каталог</div>
        <h1 className="entity-title">Жанры</h1>
        <div className="entity-stats">
          <span>
            <strong>{genres.length}</strong> жанров
          </span>
          <span className="dot">·</span>
          <span>
            <strong>{total.toLocaleString("ru")}</strong> книг
          </span>
        </div>
      </header>

      <div className="entity-tabs">
        <input
          type="search"
          className="entity-tab-filter"
          placeholder="Фильтр по жанрам…"
          value={textFilter}
          onChange={(e) => setTextFilter(e.target.value)}
          style={{ marginLeft: 0, maxWidth: "100%", flex: 1 }}
        />
      </div>

      {loading && genres.length === 0 && (
        <div className="muted small" style={{ padding: "0 18px" }}>
          Загружаю жанры…
        </div>
      )}
      {error && <div className="content-error">{error}</div>}

      <div className={stale ? "browse-refreshing" : undefined}>
      {grouped.map(([cat, items]) => (
        <section key={cat} className="genre-category">
          <h3 className="group-header">
            {categoryLabel(cat)}
            <span className="muted">{items.length}</span>
          </h3>
          <ul className="row-list">
            {items.map((g) => (
              <li
                key={g.code}
                className="row genre"
                onClick={() => onPickGenre(g.code)}
                title={g.code}
              >
                <div className="row-body">
                  <div className="row-title">{genreLabel(g.code)}</div>
                  <div className="row-sub muted">{g.code}</div>
                </div>
                <span className="row-meta">{g.count.toLocaleString("ru")}</span>
              </li>
            ))}
          </ul>
        </section>
      ))}
      </div>
    </div>
  );
}
