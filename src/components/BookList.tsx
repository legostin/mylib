import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import type { BookListItem } from "../lib/types";
import type { SelectedBook, Selection } from "../lib/selection";

type Props = {
  items: BookListItem[];
  selectedId: number | null;
  onSelect: (id: number) => void;
  selection: Selection;
  onToggleBook: (book: SelectedBook) => void;
};

export function BookList({
  items,
  selectedId,
  onSelect,
  selection,
  onToggleBook,
}: Props) {
  const parentRef = useRef<HTMLDivElement>(null);

  const v = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 56,
    overscan: 12,
  });

  if (items.length === 0) {
    return (
      <div className="book-list empty">
        <div className="empty-message">Список пуст</div>
      </div>
    );
  }

  return (
    <div ref={parentRef} className="book-list">
      <div
        style={{
          height: v.getTotalSize(),
          position: "relative",
          width: "100%",
        }}
      >
        {v.getVirtualItems().map((vi) => {
          const item = items[vi.index];
          const active = item.id === selectedId;
          const checked = selection.books.has(item.id);
          return (
            <div
              key={item.id}
              className={`row book virt${active ? " active" : ""}${checked ? " selected" : ""}`}
              style={{
                position: "absolute",
                top: 0,
                left: 0,
                right: 0,
                height: vi.size,
                transform: `translateY(${vi.start}px)`,
              }}
              onClick={() => onSelect(item.id)}
            >
              <input
                type="checkbox"
                className="select-cb"
                checked={checked}
                onChange={() =>
                  onToggleBook({
                    id: item.id,
                    libId: item.libId,
                    title: item.title,
                    authors: item.authors,
                  })
                }
                onClick={(e) => e.stopPropagation()}
                aria-label="Выбрать книгу"
              />
              <div className="row-body">
                <div className="row-title">{item.title || "(без названия)"}</div>
                <div className="row-sub">
                  <span>{item.authors || "—"}</span>
                  {item.series && (
                    <span className="series-tag">
                      {item.series}
                      {item.serNo ? ` #${item.serNo}` : ""}
                    </span>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
