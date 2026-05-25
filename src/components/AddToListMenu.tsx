import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import type { ListKind, UserList } from "../lib/types";

type Props = {
  kind: ListKind;
  refKey: string | null;
  lists: UserList[];
  onCreateList: (name: string) => Promise<UserList | null>;
  /// Called when membership changed so the parent can refresh list counts.
  /// We await this so the sidebar count is up-to-date by the time the user
  /// looks at it.
  onChanged?: () => Promise<unknown> | unknown;
  /// Compact button label override.
  label?: string;
};

export function AddToListMenu({
  kind,
  refKey,
  lists,
  onCreateList,
  onChanged,
  label,
}: Props) {
  const [open, setOpen] = useState(false);
  const [memberIds, setMemberIds] = useState<Set<number>>(new Set());
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  // Outside-click closes the popover.
  useEffect(() => {
    if (!open) return;
    const handler = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    window.addEventListener("mousedown", handler);
    return () => window.removeEventListener("mousedown", handler);
  }, [open]);

  // Refresh membership when the popover opens or the ref changes.
  useEffect(() => {
    if (!open || !refKey) return;
    let cancelled = false;
    api
      .listsContaining(kind, refKey)
      .then((ids) => {
        if (cancelled) return;
        setMemberIds(new Set(ids));
      })
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [open, kind, refKey]);

  const toggle = async (listId: number) => {
    if (!refKey) return;
    setError(null);
    try {
      if (memberIds.has(listId)) {
        await api.removeFromList(listId, kind, refKey);
        const next = new Set(memberIds);
        next.delete(listId);
        setMemberIds(next);
      } else {
        await api.addToList(listId, kind, refKey);
        const next = new Set(memberIds);
        next.add(listId);
        setMemberIds(next);
      }
      await onChanged?.();
    } catch (e) {
      setError(String(e));
    }
  };

  const submitNew = async () => {
    const name = newName.trim();
    if (!name) {
      setCreating(false);
      return;
    }
    setError(null);
    try {
      const list = await onCreateList(name);
      if (list && refKey) {
        await api.addToList(list.id, kind, refKey);
        setMemberIds(new Set([...memberIds, list.id]));
      }
      setNewName("");
      setCreating(false);
      await onChanged?.();
    } catch (e) {
      setError(String(e));
    }
  };

  const disabled = !refKey;

  return (
    <div className="add-to-list" ref={rootRef}>
      <button
        type="button"
        className="add-list-btn"
        onClick={() => setOpen((v) => !v)}
        disabled={disabled}
        title={disabled ? "Нет стабильного ключа" : "Добавить в список"}
      >
        ★ {label ?? "В список"}
      </button>
      {open && (
        <div className="popover">
          <ul className="popover-list">
            {lists.length === 0 && (
              <li className="muted small">Списков ещё нет</li>
            )}
            {lists.map((l) => (
              <li
                key={l.id}
                className={`popover-item${memberIds.has(l.id) ? " checked" : ""}`}
                onClick={() => void toggle(l.id)}
              >
                <span className="check">{memberIds.has(l.id) ? "✓" : ""}</span>
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
              type="button"
              className="popover-create"
              onClick={() => setCreating(true)}
            >
              + Новый список
            </button>
          )}
          {error && <div className="popover-error">{error}</div>}
        </div>
      )}
    </div>
  );
}
