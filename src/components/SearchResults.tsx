import type { SearchResults } from "../lib/types";
import type { SelectedBook, Selection } from "../lib/selection";

type Props = {
  results: SearchResults;
  selectedBookId: number | null;
  onPickAuthor: (id: number) => void;
  onPickSeries: (name: string) => void;
  onPickBook: (id: number) => void;
  selection: Selection;
  onToggleBook: (book: SelectedBook) => void;
  onToggleSeries: (name: string) => void;
};

export function SearchResultsView({
  results,
  selectedBookId,
  onPickAuthor,
  onPickSeries,
  onPickBook,
  selection,
  onToggleBook,
  onToggleSeries,
}: Props) {
  const nothing =
    results.authors.length === 0 &&
    results.series.length === 0 &&
    results.books.length === 0;

  return (
    <div className="search-results">
      {nothing && <div className="empty-message">Ничего не найдено</div>}

      {results.authors.length > 0 && (
        <section>
          <h3 className="group-header">
            Авторы <span className="muted">{results.authors.length}</span>
          </h3>
          <ul className="row-list">
            {results.authors.map((a) => (
              <li
                key={a.id}
                className="row author"
                onClick={() => onPickAuthor(a.id)}
              >
                <span className="row-title">{a.display}</span>
                <span className="row-meta">{a.bookCount} кн.</span>
              </li>
            ))}
          </ul>
        </section>
      )}

      {results.series.length > 0 && (
        <section>
          <h3 className="group-header">
            Серии <span className="muted">{results.series.length}</span>
          </h3>
          <ul className="row-list">
            {results.series.map((s) => {
              const checked = selection.series.has(s.name);
              return (
                <li
                  key={s.name}
                  className={`row series${checked ? " selected" : ""}`}
                  onClick={() => onPickSeries(s.name)}
                >
                  <input
                    type="checkbox"
                    className="select-cb"
                    checked={checked}
                    onChange={() => onToggleSeries(s.name)}
                    onClick={(e) => e.stopPropagation()}
                    aria-label="Выбрать серию"
                  />
                  <span className="row-title">{s.name}</span>
                  <span className="row-meta">{s.bookCount} кн.</span>
                </li>
              );
            })}
          </ul>
        </section>
      )}

      {results.books.length > 0 && (
        <section>
          <h3 className="group-header">
            Книги <span className="muted">{results.books.length}</span>
          </h3>
          <ul className="row-list">
            {results.books.map((b) => {
              const checked = selection.books.has(b.id);
              return (
                <li
                  key={b.id}
                  className={`row book${b.id === selectedBookId ? " active" : ""}${checked ? " selected" : ""}`}
                  onClick={() => onPickBook(b.id)}
                >
                  <input
                    type="checkbox"
                    className="select-cb"
                    checked={checked}
                    onChange={() =>
                      onToggleBook({
                        id: b.id,
                        libId: b.libId,
                        title: b.title,
                        authors: b.authors,
                      })
                    }
                    onClick={(e) => e.stopPropagation()}
                    aria-label="Выбрать книгу"
                  />
                  <div className="row-body">
                    <div className="row-title">
                      {b.title || "(без названия)"}
                    </div>
                    <div className="row-sub">
                      <span>{b.authors || "—"}</span>
                      {b.series && (
                        <span className="series-tag">
                          {b.series}
                          {b.serNo ? ` #${b.serNo}` : ""}
                        </span>
                      )}
                    </div>
                  </div>
                </li>
              );
            })}
          </ul>
        </section>
      )}
    </div>
  );
}
