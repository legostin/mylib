import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { AuthorHit, BookFilters } from "../lib/types";
import type { SelectedAuthor, Selection } from "../lib/selection";
import { useSWR } from "../lib/useSWR";

type Props = {
  filters: BookFilters;
  onPickAuthor: (id: number) => void;
  selection: Selection;
  onToggleAuthor: (a: SelectedAuthor) => void;
};

/// Alphabet index drill-down: letter → two-letter prefix → authors. The OPDS
/// feed uses the same three-tier shape, and on big catalogs the prefix layer
/// is the difference between a usable list and 4k+ names per letter.
export function AlphabetIndex({
  filters,
  onPickAuthor,
  selection,
  onToggleAuthor,
}: Props) {
  const [pickedLetter, setPickedLetter] = useState<string | null>(null);
  const [pickedPrefix, setPickedPrefix] = useState<string | null>(null);
  const [authorFilter, setAuthorFilter] = useState("");

  const lettersQ = useSWR<[string, number][]>(
    "list_author_letters",
    { filters },
    () => api.authorLetters(filters),
  );
  const letters = lettersQ.data
    ? sortLettersCyrillicFirst(lettersQ.data)
    : [];

  // If a filter change makes the previously picked letter disappear, drop it.
  useEffect(() => {
    if (
      pickedLetter &&
      lettersQ.data &&
      !lettersQ.data.some(([l]) => l === pickedLetter)
    ) {
      setPickedLetter(null);
      setPickedPrefix(null);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [lettersQ.data]);

  const prefixesQ = useSWR<[string, number][]>(
    "list_author_prefixes",
    { letter: pickedLetter, filters },
    () => api.authorPrefixes(pickedLetter!, filters),
    pickedLetter != null,
  );
  const prefixes = prefixesQ.data ?? [];

  const authorsQ = useSWR<AuthorHit[]>(
    "list_authors_by_letter",
    { letter: pickedPrefix, filters },
    () => api.authorsByLetter(pickedPrefix!, filters),
    pickedPrefix != null,
  );
  const authors = authorsQ.data ?? [];

  // Reset transient UI when changing letters.
  useEffect(() => {
    setPickedPrefix(null);
    setAuthorFilter("");
  }, [pickedLetter]);

  const loading = lettersQ.loading || prefixesQ.loading || authorsQ.loading;
  const refreshing = lettersQ.stale || prefixesQ.stale || authorsQ.stale;
  const error = lettersQ.error || prefixesQ.error || authorsQ.error;

  const filteredAuthors = authorFilter.trim()
    ? authors.filter((a) =>
        a.display.toLowerCase().includes(authorFilter.trim().toLowerCase()),
      )
    : authors;

  // Capitalize first letter, lowercase the rest for tidy "Аа Аб Ав".
  const formatPrefix = (p: string) =>
    p.length <= 1 ? p : p[0] + p.slice(1).toLowerCase();

  return (
    <div className="alphabet-index">
      <div className="alphabet-trail">
        <button
          className={`alphabet-crumb${!pickedLetter ? " active" : ""}`}
          onClick={() => {
            setPickedLetter(null);
            setPickedPrefix(null);
          }}
        >
          А–Я
        </button>
        {pickedLetter && (
          <>
            <span className="alphabet-trail-sep">›</span>
            <button
              className={`alphabet-crumb${!pickedPrefix ? " active" : ""}`}
              onClick={() => setPickedPrefix(null)}
            >
              {pickedLetter}
            </button>
          </>
        )}
        {pickedPrefix && (
          <>
            <span className="alphabet-trail-sep">›</span>
            <span className="alphabet-crumb active">
              {formatPrefix(pickedPrefix)}
            </span>
          </>
        )}
      </div>

      {!pickedLetter && (
        <div className={`alphabet-letters${refreshing ? " refreshing" : ""}`}>
          {loading && letters.length === 0 && (
            <span className="muted small">Загружаю…</span>
          )}
          {!loading && letters.length === 0 && (
            <span className="muted small">
              Импортируйте каталог или ослабьте фильтры
            </span>
          )}
          {letters.map(([lt, cnt]) => (
            <button
              key={lt}
              className="alphabet-letter"
              onClick={() => setPickedLetter(lt)}
              title={`${cnt.toLocaleString("ru")} авторов`}
            >
              <span className="alphabet-letter-char">{lt}</span>
              <span className="alphabet-letter-count">
                {cnt.toLocaleString("ru")}
              </span>
            </button>
          ))}
        </div>
      )}

      {pickedLetter && !pickedPrefix && (
        <div className={`alphabet-letters${refreshing ? " refreshing" : ""}`}>
          {loading && prefixes.length === 0 && (
            <span className="muted small">Загружаю…</span>
          )}
          <button
            className="alphabet-letter"
            onClick={() => setPickedPrefix(pickedLetter)}
            title="Все авторы на эту букву"
          >
            <span className="alphabet-letter-char">{pickedLetter}…</span>
            <span className="alphabet-letter-count">все</span>
          </button>
          {prefixes.map(([pfx, cnt]) => (
            <button
              key={pfx}
              className="alphabet-letter"
              onClick={() => setPickedPrefix(pfx)}
              title={`${cnt.toLocaleString("ru")} авторов`}
            >
              <span className="alphabet-letter-char">{formatPrefix(pfx)}</span>
              <span className="alphabet-letter-count">
                {cnt.toLocaleString("ru")}
              </span>
            </button>
          ))}
          {!loading && prefixes.length === 0 && (
            <span className="muted small">
              На «{pickedLetter}» нет авторов под текущими фильтрами
            </span>
          )}
        </div>
      )}

      {pickedPrefix && (
        <div className="alphabet-authors">
          <div className="alphabet-authors-head">
            <h3>Авторы на «{formatPrefix(pickedPrefix)}»</h3>
            <input
              type="search"
              className="alphabet-authors-filter"
              placeholder="Фильтр по имени…"
              value={authorFilter}
              onChange={(e) => setAuthorFilter(e.target.value)}
            />
          </div>
          {loading && authors.length === 0 && (
            <div className="muted small">Загружаю авторов…</div>
          )}
          {!loading && filteredAuthors.length === 0 && (
            <div className="muted small">
              {authors.length === 0 ? "Нет авторов" : "Не подошло под фильтр"}
            </div>
          )}
          <ul className="row-list">
            {filteredAuthors.map((a) => {
              const checked = selection.authors.has(a.id);
              return (
                <li
                  key={a.id}
                  className={`row author${checked ? " selected" : ""}`}
                  onClick={() => onPickAuthor(a.id)}
                >
                  <input
                    type="checkbox"
                    className="select-cb"
                    checked={checked}
                    onChange={() =>
                      onToggleAuthor({ id: a.id, display: a.display })
                    }
                    onClick={(e) => e.stopPropagation()}
                    title="Выбрать автора"
                  />
                  <div className="row-body">
                    <div className="row-title">{a.display}</div>
                  </div>
                  <span className="row-meta">{a.bookCount}</span>
                </li>
              );
            })}
          </ul>
        </div>
      )}

      {error && <div className="content-error">{error}</div>}
    </div>
  );
}

/// Reorder `[letter, count]` rows so Cyrillic letters appear first, then
/// Latin, then anything else (digits, punctuation). Inside each bucket the
/// natural locale order is preserved. SQLite's NOCASE collation orders by
/// codepoint, which puts Cyrillic *after* Latin — undesirable for a
/// Russian-first catalog.
function sortLettersCyrillicFirst(
  rows: [string, number][],
): [string, number][] {
  const bucket = (ch: string): number => {
    const code = ch.charCodeAt(0);
    if (code >= 0x0400 && code <= 0x04ff) return 0; // Cyrillic
    if ((code >= 0x41 && code <= 0x5a) || (code >= 0x61 && code <= 0x7a))
      return 1; // Latin
    return 2; // digits / other
  };
  return [...rows].sort((a, b) => {
    const ba = bucket(a[0]);
    const bb = bucket(b[0]);
    if (ba !== bb) return ba - bb;
    return a[0].localeCompare(b[0], "ru");
  });
}
