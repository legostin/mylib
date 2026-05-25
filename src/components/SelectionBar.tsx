import { useEffect, useRef, useState } from "react";
import type { Selection } from "../lib/selection";
import { selectionSize } from "../lib/selection";
import type { UserList } from "../lib/types";

type Props = {
  selection: Selection;
  lists: UserList[];
  onCreateList: (name: string) => Promise<UserList | null>;
  onBulkAddToList: (listId: number) => Promise<void> | void;
  onShow: () => void;
  onExport: () => void | Promise<void>;
  onClear: () => void;
};

export function SelectionBar({
  selection,
  lists,
  onCreateList,
  onBulkAddToList,
  onShow,
  onExport,
  onClear,
}: Props) {
  if (selectionSize(selection) === 0) return null;

  const parts: string[] = [];
  if (selection.books.size > 0)
    parts.push(
      `${selection.books.size} ${plural(selection.books.size, "книга", "книги", "книг")}`,
    );
  if (selection.authors.size > 0)
    parts.push(
      `${selection.authors.size} ${plural(selection.authors.size, "автор", "автора", "авторов")}`,
    );
  if (selection.series.size > 0)
    parts.push(
      `${selection.series.size} ${plural(selection.series.size, "серия", "серии", "серий")}`,
    );

  return (
    <div className="selection-bar">
      <button
        className="selection-summary link"
        onClick={onShow}
        title="Показать выбранное"
      >
        Выбрано: {parts.join(", ")}
      </button>
      <BulkAddToListMenu
        lists={lists}
        onCreateList={onCreateList}
        onBulkAddToList={onBulkAddToList}
      />
      <button className="primary" onClick={() => void onExport()}>
        ⬇ Экспорт выбранного
      </button>
      <button onClick={onClear}>Очистить</button>
    </div>
  );
}

function BulkAddToListMenu({
  lists,
  onCreateList,
  onBulkAddToList,
}: {
  lists: UserList[];
  onCreateList: (name: string) => Promise<UserList | null>;
  onBulkAddToList: (listId: number) => Promise<void> | void;
}) {
  const [open, setOpen] = useState(false);
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [busy, setBusy] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [open]);

  const handleAdd = async (id: number) => {
    setBusy(true);
    try {
      await onBulkAddToList(id);
    } finally {
      setBusy(false);
      setOpen(false);
    }
  };

  const submitNew = async () => {
    const name = newName.trim();
    setCreating(false);
    setNewName("");
    if (!name) return;
    setBusy(true);
    try {
      const created = await onCreateList(name);
      if (created) await onBulkAddToList(created.id);
    } finally {
      setBusy(false);
      setOpen(false);
    }
  };

  return (
    <div className="add-to-list" ref={rootRef}>
      <button
        className="add-list-btn"
        onClick={() => setOpen((v) => !v)}
        disabled={busy}
      >
        ★ В список
      </button>
      {open && (
        <div className="popover popover-up">
          <ul className="popover-list">
            {lists.length === 0 && (
              <li className="muted small">Списков ещё нет</li>
            )}
            {lists.map((l) => (
              <li
                key={l.id}
                className="popover-item"
                onClick={() => void handleAdd(l.id)}
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
                if (e.key === "Enter") void submitNew();
                if (e.key === "Escape") {
                  setNewName("");
                  setCreating(false);
                }
              }}
              onBlur={() => void submitNew()}
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
  );
}

function plural(n: number, one: string, few: string, many: string): string {
  const mod10 = n % 10;
  const mod100 = n % 100;
  if (mod10 === 1 && mod100 !== 11) return one;
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) return few;
  return many;
}
