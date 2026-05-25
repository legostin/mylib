import type { ListContents } from "../lib/types";
import type { SelectedBook, Selection } from "../lib/selection";

type Props = {
  contents: ListContents;
  selectedBookId: number | null;
  onPickBook: (id: number) => void;
  onPickAuthor: (id: number) => void;
  onPickSeries: (name: string) => void;
  onRemoveOrphan: (kind: "book" | "author" | "series", refKey: string) => void;
  selection: Selection;
  onToggleBook: (book: SelectedBook) => void;
  onToggleSeries: (name: string) => void;
};

export function ListPage({
  contents,
  selectedBookId,
  onPickBook,
  onPickAuthor,
  onPickSeries,
  onRemoveOrphan,
  selection,
  onToggleBook,
  onToggleSeries,
}: Props) {
  const { list, books, authors, series, orphans } = contents;
  const empty =
    books.length === 0 &&
    authors.length === 0 &&
    series.length === 0 &&
    orphans.length === 0;

  return (
    <div className="list-page">
      <div className="page-head">
        <h2>{list.name}</h2>
        <span className="muted">{list.itemCount}</span>
      </div>

      {empty && (
        <div className="empty-message">
          Список пуст. Откройте автора, серию или книгу и нажмите{" "}
          <em>★ В список</em>.
        </div>
      )}

      {authors.length > 0 && (
        <section>
          <h3 className="group-header">
            Авторы <span className="muted">{authors.length}</span>
          </h3>
          <ul className="row-list">
            {authors.map((a) => (
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

      {series.length > 0 && (
        <section>
          <h3 className="group-header">
            Серии <span className="muted">{series.length}</span>
          </h3>
          <ul className="row-list">
            {series.map((s) => {
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

      {books.length > 0 && (
        <section>
          <h3 className="group-header">
            Книги <span className="muted">{books.length}</span>
          </h3>
          <ul className="row-list">
            {books.map((b) => {
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

      {orphans.length > 0 && (
        <section>
          <h3 className="group-header">
            Не найдено <span className="muted">{orphans.length}</span>
          </h3>
          <ul className="row-list">
            {orphans.map((o, i) => (
              <li key={i} className="row orphan">
                <span className="row-title muted">
                  [{o.kind}] {o.refKey}
                </span>
                <button
                  className="row-meta link"
                  onClick={() => onRemoveOrphan(o.kind, o.refKey)}
                >
                  убрать
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
    </div>
  );
}
