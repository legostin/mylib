import { useMemo, useState } from "react";
import type { AuthorView, UserList } from "../lib/types";
import type { SelectedBook, Selection } from "../lib/selection";
import { AddToListMenu } from "./AddToListMenu";

type Tab = "series" | "all" | "loose";

type Props = {
  author: AuthorView;
  selectedBookId: number | null;
  lists: UserList[];
  onCreateList: (name: string) => Promise<UserList | null>;
  onListsChanged: () => void;
  onPickBook: (id: number) => void;
  onPickSeries: (name: string) => void;
  onExportAll?: () => void | Promise<void>;
  selection: Selection;
  onToggleBook: (book: SelectedBook) => void;
  onToggleSeries: (name: string) => void;
};

export function AuthorPage({
  author,
  selectedBookId,
  lists,
  onCreateList,
  onListsChanged,
  onPickBook,
  onPickSeries,
  onExportAll,
  selection,
  onToggleBook,
  onToggleSeries,
}: Props) {
  const [filter, setFilter] = useState("");
  const [tab, setTab] = useState<Tab>("series");

  // Stats per tab don't change with the filter — they describe the catalog,
  // not the filtered view.
  const groupsWithSeries = author.groups.filter((g) => g.name);
  const groupsLoose = author.groups.filter((g) => !g.name);
  const totalBooks = author.groups.reduce((n, g) => n + g.books.length, 0);
  const looseBooks = groupsLoose.reduce((n, g) => n + g.books.length, 0);

  const visibleGroups = useMemo(() => {
    let base: typeof author.groups;
    if (tab === "series") base = groupsWithSeries;
    else if (tab === "loose") base = groupsLoose;
    else base = author.groups;

    const q = filter.trim().toLowerCase();
    if (!q) return base;
    return base
      .map((g) => {
        const seriesMatches = g.name?.toLowerCase().includes(q) ?? false;
        return {
          ...g,
          books: seriesMatches
            ? g.books
            : g.books.filter((b) =>
                (b.title ?? "").toLowerCase().includes(q),
              ),
        };
      })
      .filter((g) => g.books.length > 0);
  }, [author.groups, groupsWithSeries, groupsLoose, tab, filter]);

  return (
    <div className="entity-page author-page">
      <header className="entity-hero">
        <div className="section-label">Автор</div>
        <h1 className="entity-title">{author.display}</h1>
        <div className="entity-stats">
          <span>
            <strong>{totalBooks}</strong> книг
          </span>
          <span className="dot">·</span>
          <span>
            <strong>{groupsWithSeries.length}</strong> серий
          </span>
          {looseBooks > 0 && (
            <>
              <span className="dot">·</span>
              <span>
                <strong>{looseBooks}</strong> вне серий
              </span>
            </>
          )}
        </div>
        <div className="entity-actions">
          <AddToListMenu
            kind="author"
            refKey={author.display}
            lists={lists}
            onCreateList={onCreateList}
            onChanged={onListsChanged}
          />
          {onExportAll && (
            <button
              className="entity-btn"
              onClick={() => void onExportAll()}
              title="Экспортировать все книги автора"
            >
              ⬇ Экспорт всего
            </button>
          )}
        </div>
      </header>

      <div className="entity-tabs">
        <button
          className={`entity-tab${tab === "series" ? " active" : ""}`}
          onClick={() => setTab("series")}
        >
          Серии{" "}
          <span className="entity-tab-count">{groupsWithSeries.length}</span>
        </button>
        <button
          className={`entity-tab${tab === "all" ? " active" : ""}`}
          onClick={() => setTab("all")}
        >
          Все книги <span className="entity-tab-count">{totalBooks}</span>
        </button>
        {looseBooks > 0 && (
          <button
            className={`entity-tab${tab === "loose" ? " active" : ""}`}
            onClick={() => setTab("loose")}
          >
            Без серий <span className="entity-tab-count">{looseBooks}</span>
          </button>
        )}
        <input
          type="search"
          className="entity-tab-filter"
          placeholder="Фильтр…"
          value={filter}
          onChange={(e) => setFilter(e.target.value)}
        />
      </div>

      {visibleGroups.length === 0 && (
        <div className="empty-message">Ничего не подошло под фильтр</div>
      )}

      {visibleGroups.map((g, i) => {
        const seriesChecked = g.name ? selection.series.has(g.name) : false;
        return (
          <section
            key={(g.name ?? "__no_series__") + i}
            className="series-section"
          >
            {tab !== "all" && (
              <h3 className="group-header">
                {g.name ? (
                  <>
                    <input
                      type="checkbox"
                      className="select-cb"
                      checked={seriesChecked}
                      onChange={() => onToggleSeries(g.name!)}
                      onClick={(e) => e.stopPropagation()}
                      aria-label="Выбрать всю серию"
                      title="Выбрать всю серию"
                    />
                    <button
                      className="link"
                      onClick={() => onPickSeries(g.name!)}
                      title="Открыть серию"
                    >
                      {g.name}
                    </button>
                  </>
                ) : (
                  <span className="muted">Вне серий</span>
                )}
                <span className="muted">{g.books.length}</span>
              </h3>
            )}
            <ul className="row-list">
              {g.books.map((b) => {
                const checked = selection.books.has(b.id);
                return (
                  <li
                    key={b.id}
                    className={`row book${
                      b.id === selectedBookId ? " active" : ""
                    }${checked ? " selected" : ""}`}
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
                        {b.serNo && tab !== "all" ? (
                          <span className="ser-no">#{b.serNo}</span>
                        ) : null}
                        {b.title || "(без названия)"}
                      </div>
                      <div className="row-sub muted">
                        {tab === "all" && b.series && (
                          <span className="series-tag">
                            {b.series}
                            {b.serNo ? ` #${b.serNo}` : ""}
                          </span>
                        )}
                        {b.lang && <span>{b.lang}</span>}
                        {b.size > 0 && (
                          <span>{Math.round(b.size / 1024)} КБ</span>
                        )}
                      </div>
                    </div>
                  </li>
                );
              })}
            </ul>
          </section>
        );
      })}
    </div>
  );
}
