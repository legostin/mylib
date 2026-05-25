import { useEffect, useRef, useState } from "react";
import type { Selection } from "../lib/selection";
import { selectionSize } from "../lib/selection";
import type { UserList } from "../lib/types";

type Props = {
  selection: Selection;
  lists: UserList[];
  onCreateList: (name: string) => Promise<UserList | null>;
  onBulkAddToList: (listId: number) => Promise<void> | void;
  onRemoveBook: (id: number) => void;
  onRemoveSeries: (name: string) => void;
  onRemoveAuthor: (id: number) => void;
  onPickBook: (id: number) => void;
  onPickSeries: (name: string) => void;
  onPickAuthor: (id: number) => void;
  onExport: () => Promise<void> | void;
  onClear: () => void;
  onClose: () => void;
};

export function SelectionDrawer({
  selection,
  lists,
  onCreateList,
  onBulkAddToList,
  onRemoveBook,
  onRemoveSeries,
  onRemoveAuthor,
  onPickBook,
  onPickSeries,
  onPickAuthor,
  onExport,
  onClear,
  onClose,
}: Props) {
  const books = [...selection.books.values()];
  const series = [...selection.series];
  const authors = [...selection.authors.values()];
  const [listMenuOpen, setListMenuOpen] = useState(false);
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [busyList, setBusyList] = useState<number | null>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!listMenuOpen) return;
    const handler = (e: MouseEvent) => {
      if (!menuRef.current?.contains(e.target as Node)) {
        setListMenuOpen(false);
      }
    };
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [listMenuOpen]);

  // Close drawer on Esc.
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [onClose]);

  const handleBulkAdd = async (listId: number) => {
    setBusyList(listId);
    try {
      await onBulkAddToList(listId);
    } finally {
      setBusyList(null);
      setListMenuOpen(false);
    }
  };

  const submitNewList = async () => {
    const name = newName.trim();
    setCreating(false);
    setNewName("");
    if (!name) return;
    const created = await onCreateList(name);
    if (created) await handleBulkAdd(created.id);
  };

  return (
    <div className="drawer-backdrop" onMouseDown={onClose}>
      <aside
        className="selection-drawer"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <header className="drawer-head">
          <h2>Выбрано: {selectionSize(selection)}</h2>
          <button className="icon-btn" onClick={onClose} title="Закрыть">
            ×
          </button>
        </header>

        <div className="drawer-actions">
          <button className="primary" onClick={() => void onExport()}>
            ⬇ Экспорт
          </button>
          <div className="add-to-list" ref={menuRef}>
            <button
              className="add-list-btn"
              onClick={() => setListMenuOpen((v) => !v)}
            >
              ★ В список
            </button>
            {listMenuOpen && (
              <div className="popover">
                <ul className="popover-list">
                  {lists.length === 0 && (
                    <li className="muted small">Списков ещё нет</li>
                  )}
                  {lists.map((l) => (
                    <li
                      key={l.id}
                      className={`popover-item${busyList === l.id ? " busy" : ""}`}
                      onClick={() => void handleBulkAdd(l.id)}
                    >
                      <span className="check" />
                      <span>{l.name}</span>
                    </li>
                  ))}
                </ul>
                <div className="popover-divider" />
                {creating ? (
                  <input
                    type="text"
                    autoFocus
                    placeholder="Название нового списка…"
                    value={newName}
                    onChange={(e) => setNewName(e.target.value)}
                    onKeyDown={(e) => {
                      if (e.key === "Enter") void submitNewList();
                      if (e.key === "Escape") {
                        setNewName("");
                        setCreating(false);
                      }
                    }}
                    onBlur={() => void submitNewList()}
                  />
                ) : (
                  <button
                    className="popover-create"
                    onClick={() => setCreating(true)}
                  >
                    + Новый список
                  </button>
                )}
              </div>
            )}
          </div>
          <button onClick={onClear}>Очистить всё</button>
        </div>

        <div className="drawer-body">
          {authors.length > 0 && (
            <section>
              <h3 className="group-header">
                Авторы <span className="muted">{authors.length}</span>
              </h3>
              <ul className="row-list">
                {authors.map((a) => (
                  <li key={a.id} className="row author">
                    <span
                      className="row-title link"
                      onClick={() => {
                        onPickAuthor(a.id);
                        onClose();
                      }}
                    >
                      {a.display}
                    </span>
                    <button
                      className="row-meta link"
                      onClick={() => onRemoveAuthor(a.id)}
                      title="Убрать из выбора"
                    >
                      убрать
                    </button>
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
                {series.map((name) => (
                  <li key={name} className="row series">
                    <span
                      className="row-title link"
                      onClick={() => {
                        onPickSeries(name);
                        onClose();
                      }}
                    >
                      {name}
                    </span>
                    <button
                      className="row-meta link"
                      onClick={() => onRemoveSeries(name)}
                      title="Убрать из выбора"
                    >
                      убрать
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          )}
          {books.length > 0 && (
            <section>
              <h3 className="group-header">
                Книги <span className="muted">{books.length}</span>
              </h3>
              <ul className="row-list">
                {books.map((b) => (
                  <li key={b.id} className="row book">
                    <div
                      className="row-body link"
                      onClick={() => {
                        onPickBook(b.id);
                        onClose();
                      }}
                    >
                      <div className="row-title">
                        {b.title || "(без названия)"}
                      </div>
                      {b.authors && (
                        <div className="row-sub muted">{b.authors}</div>
                      )}
                    </div>
                    <button
                      className="row-meta link"
                      onClick={() => onRemoveBook(b.id)}
                      title="Убрать из выбора"
                    >
                      убрать
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          )}
        </div>
      </aside>
    </div>
  );
}
