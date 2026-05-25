import { useMemo, useState } from "react";
import type { BookListItem, UserList } from "../lib/types";
import type { SelectedBook, Selection } from "../lib/selection";
import { AddToListMenu } from "./AddToListMenu";

type Props = {
  name: string;
  books: BookListItem[];
  selectedBookId: number | null;
  lists: UserList[];
  onCreateList: (name: string) => Promise<UserList | null>;
  onListsChanged: () => void;
  onPickBook: (id: number) => void;
  selection: Selection;
  onToggleBook: (book: SelectedBook) => void;
};

export function SeriesPage({
  name,
  books,
  selectedBookId,
  lists,
  onCreateList,
  onListsChanged,
  onPickBook,
  selection,
  onToggleBook,
}: Props) {
  const [filter, setFilter] = useState("");
  const visibleBooks = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return books;
    return books.filter(
      (b) =>
        (b.title ?? "").toLowerCase().includes(q) ||
        (b.authors ?? "").toLowerCase().includes(q),
    );
  }, [books, filter]);

  return (
    <div className="series-page">
      <div className="page-head">
        <h2>{name}</h2>
        <span className="muted">
          {filter.trim() && visibleBooks.length !== books.length
            ? `${visibleBooks.length} из ${books.length} книг`
            : `${books.length} книг`}
        </span>
        <input
          type="search"
          className="page-filter"
          placeholder="Фильтр по названиям…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
        <div className="page-actions">
          <AddToListMenu
            kind="series"
            refKey={name}
            lists={lists}
            onCreateList={onCreateList}
            onChanged={onListsChanged}
          />
        </div>
      </div>
      <ul className="row-list">
        {visibleBooks.map((b) => {
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
                  {b.serNo ? <span className="ser-no">#{b.serNo}</span> : null}
                  {b.title || "(без названия)"}
                </div>
                <div className="row-sub muted">{b.authors || "—"}</div>
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
