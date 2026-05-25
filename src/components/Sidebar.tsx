import { useState } from "react";
import type { CollectionInfo, LibraryStats, UserList } from "../lib/types";
import { ShareBar } from "./ShareBar";

export type SidebarSection = "library" | "authors" | "series" | "genres" | "languages";

type Props = {
  collection: CollectionInfo | null;
  stats: LibraryStats | null;
  lists: UserList[];
  activeListId: number | null;
  activeSection: SidebarSection;
  onSelectSection: (section: SidebarSection) => void;
  onSelectList: (id: number) => void;
  onCreateList: (name: string) => Promise<unknown> | unknown;
  onDeleteList: (id: number) => Promise<void> | void;
  onOpen: () => void;
  onAbout: () => void;
  busy: boolean;
};

export function Sidebar({
  collection,
  stats,
  lists,
  activeListId,
  activeSection,
  onSelectSection,
  onSelectList,
  onCreateList,
  onDeleteList,
  onOpen,
  onAbout,
  busy,
}: Props) {
  const empty = !stats || stats.books === 0;
  const [adding, setAdding] = useState(false);
  const [newName, setNewName] = useState("");

  // Sidebar nav: top-level catalog entries. Books count lives down in the
  // stats block; for Авторы/Серии we surface their totals inline since those
  // are the entry points users actually browse from.
  const navItems: {
    key: SidebarSection;
    label: string;
    icon: string;
    count?: number;
  }[] = [
    { key: "library", label: "Библиотека", icon: "▦" },
    { key: "authors", label: "Авторы", icon: "✎", count: stats?.authors },
    { key: "series", label: "Серии", icon: "≡", count: stats?.series },
    { key: "genres", label: "Жанры", icon: "✦" },
    { key: "languages", label: "Языки", icon: "⊕" },
  ];

  const submitNew = async () => {
    const name = newName.trim();
    if (!name) {
      setAdding(false);
      return;
    }
    await onCreateList(name);
    setNewName("");
    setAdding(false);
  };

  return (
    <aside className="sidebar">
      <div className="sidebar-head">
        <h1>MyLib</h1>
        <button
          type="button"
          className="icon-btn about-trigger"
          onClick={onAbout}
          title="О программе"
          aria-label="О программе"
        >
          ?
        </button>
      </div>
      <button className="primary" onClick={onOpen} disabled={busy}>
        {empty ? "Открыть INPX…" : "Заменить INPX…"}
      </button>
      {collection?.name ? (
        <div className="collection">
          <div className="collection-name" title={collection.name}>
            {collection.name}
          </div>
          {collection.version && (
            <div className="collection-version">v{collection.version}</div>
          )}
        </div>
      ) : (
        <p className="hint">
          Выберите .inpx-файл, чтобы проиндексировать каталог.
        </p>
      )}

      <nav className="sidebar-nav">
        {navItems.map((it) => (
          <button
            key={it.key}
            type="button"
            className={`sidebar-nav-item${
              activeSection === it.key && activeListId == null ? " active" : ""
            }`}
            onClick={() => onSelectSection(it.key)}
            disabled={empty}
          >
            <span className="sidebar-nav-icon" aria-hidden="true">
              {it.icon}
            </span>
            <span className="sidebar-nav-label">{it.label}</span>
            {it.count != null && (
              <span className="sidebar-nav-count">
                {it.count.toLocaleString("ru")}
              </span>
            )}
          </button>
        ))}
      </nav>

      <div className="lists">
        <div className="lists-head">
          <span>Списки</span>
          <button
            type="button"
            className="icon-btn"
            title="Новый список"
            onClick={() => setAdding(true)}
          >
            +
          </button>
        </div>
        <ul className="list-items">
          {lists.map((l) => {
            const cls = [
              "list-item",
              activeListId === l.id ? "active" : "",
              !l.builtin ? "deletable" : "",
            ]
              .filter(Boolean)
              .join(" ");
            return (
              <li
                key={l.id}
                className={cls}
                onClick={() => onSelectList(l.id)}
              >
                <span className="list-name">{l.name}</span>
                <span className="list-tail">
                  <span className="num">{l.itemCount}</span>
                  {!l.builtin && (
                    <button
                      className="cross"
                      title="Удалить список"
                      onClick={(e) => {
                        e.stopPropagation();
                        void onDeleteList(l.id);
                      }}
                    >
                      ×
                    </button>
                  )}
                </span>
              </li>
            );
          })}
          {adding && (
            <li className="list-item adding">
              <input
                type="text"
                autoFocus
                placeholder="Название…"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void submitNew();
                  if (e.key === "Escape") {
                    setNewName("");
                    setAdding(false);
                  }
                }}
                onBlur={() => void submitNew()}
              />
            </li>
          )}
        </ul>
      </div>

      {stats && (
        <ul className="stats">
          <li>
            <span>Книги</span>
            <strong>{stats.books.toLocaleString("ru")}</strong>
          </li>
          <li>
            <span>Авторы</span>
            <strong>{stats.authors.toLocaleString("ru")}</strong>
          </li>
          <li>
            <span>Серии</span>
            <strong>{stats.series.toLocaleString("ru")}</strong>
          </li>
        </ul>
      )}
      {collection?.booksDir && (
        <div className="books-dir" title={collection.booksDir}>
          <span className="muted">Архивы:</span>{" "}
          <code>{shorten(collection.booksDir)}</code>
        </div>
      )}
      <ShareBar disabled={empty || busy} />
    </aside>
  );
}

function shorten(p: string): string {
  if (p.length <= 32) return p;
  return "…" + p.slice(-31);
}
