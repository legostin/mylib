import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { api } from "../lib/api";
import type { Book, ReaderBook, TocEntry } from "../lib/types";

type Props = {
  book: Book;
  onClose: () => void;
};

type ReaderTheme = "light" | "sepia" | "dark";
type ReaderFont = "serif" | "sans" | "mono";

type Prefs = {
  theme: ReaderTheme;
  font: ReaderFont;
  fontSize: number; // px
};

const PREFS_KEY = "mylib.reader.prefs";
const DEFAULT_PREFS: Prefs = { theme: "light", font: "serif", fontSize: 18 };

function loadPrefs(): Prefs {
  try {
    const raw = localStorage.getItem(PREFS_KEY);
    if (!raw) return DEFAULT_PREFS;
    const p = JSON.parse(raw) as Partial<Prefs>;
    return {
      theme: p.theme ?? DEFAULT_PREFS.theme,
      font: p.font ?? DEFAULT_PREFS.font,
      fontSize: Math.min(
        32,
        Math.max(12, Number(p.fontSize) || DEFAULT_PREFS.fontSize),
      ),
    };
  } catch {
    return DEFAULT_PREFS;
  }
}

function savePrefs(p: Prefs) {
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify(p));
  } catch {
    /* quota: ignore */
  }
}

export function Reader({ book, onClose }: Props) {
  const [data, setData] = useState<ReaderBook | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [activeChapter, setActiveChapter] = useState<string | null>(null);
  const [tocOpen, setTocOpen] = useState(true);
  const [prefs, setPrefs] = useState<Prefs>(() => loadPrefs());

  const scrollRef = useRef<HTMLDivElement | null>(null);
  const chapterRefs = useRef<Map<string, HTMLElement>>(new Map());
  const persistTimer = useRef<number | null>(null);

  // Load book once on mount.
  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    api
      .getReaderBook(book.id)
      .then((rb) => {
        if (cancelled) return;
        setData(rb);
        const startId =
          rb.position?.chapterId ??
          (rb.chapters.length > 0 ? rb.chapters[0].id : null);
        setActiveChapter(startId);
      })
      .catch((e) => !cancelled && setError(String(e)))
      .finally(() => !cancelled && setLoading(false));
    return () => {
      cancelled = true;
    };
  }, [book.id]);

  // Restore scroll once chapter is mounted.
  useEffect(() => {
    if (!data || !activeChapter || !scrollRef.current) return;
    const pos = data.position;
    if (!pos || pos.chapterId !== activeChapter) return;
    // Defer until DOM has measured the chapter.
    const el = chapterRefs.current.get(activeChapter);
    if (!el) return;
    const scroller = scrollRef.current;
    const offsetTop = el.offsetTop;
    const chapterHeight = el.scrollHeight;
    scroller.scrollTop = offsetTop + chapterHeight * Math.max(0, Math.min(1, pos.scroll));
    // Only restore on first mount of this chapter, not every change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [data, activeChapter]);

  // Persist prefs.
  useEffect(() => {
    savePrefs(prefs);
  }, [prefs]);

  // Track active chapter while scrolling.
  const onScroll = useCallback(() => {
    const scroller = scrollRef.current;
    if (!scroller || !data) return;
    const middle = scroller.scrollTop + scroller.clientHeight / 3;
    let current: string | null = null;
    for (const ch of data.chapters) {
      const el = chapterRefs.current.get(ch.id);
      if (!el) continue;
      if (el.offsetTop <= middle) current = ch.id;
      else break;
    }
    if (current && current !== activeChapter) setActiveChapter(current);

    if (book.lib_id && current) {
      // Debounce position write to ~600ms.
      if (persistTimer.current) window.clearTimeout(persistTimer.current);
      const chapId = current;
      const el = chapterRefs.current.get(chapId);
      if (el) {
        const within = Math.max(
          0,
          Math.min(1, (scroller.scrollTop - el.offsetTop) / Math.max(1, el.scrollHeight)),
        );
        persistTimer.current = window.setTimeout(() => {
          api.saveReadingPosition(book.lib_id, chapId, within).catch(() => {
            /* offline: ignore */
          });
        }, 600);
      }
    }
  }, [data, activeChapter, book.lib_id]);

  // Keyboard nav.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      if (!data) return;
      const idx = data.chapters.findIndex((c) => c.id === activeChapter);
      if (idx < 0) return;
      if (
        (e.key === "ArrowRight" || e.key === "PageDown") &&
        idx < data.chapters.length - 1 &&
        !inEditable(e.target)
      ) {
        e.preventDefault();
        jumpTo(data.chapters[idx + 1].id);
      } else if (
        (e.key === "ArrowLeft" || e.key === "PageUp") &&
        idx > 0 &&
        !inEditable(e.target)
      ) {
        e.preventDefault();
        jumpTo(data.chapters[idx - 1].id);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [data, activeChapter, onClose]);

  const jumpTo = useCallback((chapterId: string, anchor?: string | null) => {
    setActiveChapter(chapterId);
    requestAnimationFrame(() => {
      const el = chapterRefs.current.get(chapterId);
      if (!el || !scrollRef.current) return;
      if (anchor) {
        const target = el.querySelector<HTMLElement>(
          `#${cssEscape(anchor)}`,
        );
        if (target) {
          scrollRef.current.scrollTop =
            target.getBoundingClientRect().top -
            scrollRef.current.getBoundingClientRect().top +
            scrollRef.current.scrollTop;
          return;
        }
      }
      scrollRef.current.scrollTop = el.offsetTop;
    });
  }, []);

  const currentIdx = useMemo(() => {
    if (!data) return -1;
    return data.chapters.findIndex((c) => c.id === activeChapter);
  }, [data, activeChapter]);

  const themeClass = `reader-theme-${prefs.theme}`;
  const fontClass = `reader-font-${prefs.font}`;

  return (
    <div className={`reader-overlay ${themeClass}`}>
      <div className="reader-toolbar">
        <button
          className="reader-btn"
          onClick={() => setTocOpen((v) => !v)}
          title="Оглавление"
        >
          ☰
        </button>
        <div className="reader-title" title={data?.title ?? book.title}>
          {data?.title || book.title || "(без названия)"}
        </div>
        <div className="reader-spacer" />
        <div className="reader-controls">
          <button
            className="reader-btn"
            disabled={!data || currentIdx <= 0}
            onClick={() =>
              data && currentIdx > 0 && jumpTo(data.chapters[currentIdx - 1].id)
            }
            title="Предыдущая глава"
          >
            ‹
          </button>
          <span className="reader-ch-count">
            {data && currentIdx >= 0
              ? `${currentIdx + 1}/${data.chapters.length}`
              : ""}
          </span>
          <button
            className="reader-btn"
            disabled={
              !data || currentIdx < 0 || currentIdx >= data.chapters.length - 1
            }
            onClick={() =>
              data &&
              currentIdx < data.chapters.length - 1 &&
              jumpTo(data.chapters[currentIdx + 1].id)
            }
            title="Следующая глава"
          >
            ›
          </button>
          <div className="reader-divider" />
          <button
            className="reader-btn"
            onClick={() =>
              setPrefs((p) => ({ ...p, fontSize: Math.max(12, p.fontSize - 1) }))
            }
            title="Меньше шрифт"
          >
            A−
          </button>
          <button
            className="reader-btn"
            onClick={() =>
              setPrefs((p) => ({ ...p, fontSize: Math.min(32, p.fontSize + 1) }))
            }
            title="Больше шрифт"
          >
            A+
          </button>
          <select
            className="reader-select"
            value={prefs.font}
            onChange={(e) =>
              setPrefs((p) => ({ ...p, font: e.target.value as ReaderFont }))
            }
            title="Шрифт"
          >
            <option value="serif">Serif</option>
            <option value="sans">Sans</option>
            <option value="mono">Mono</option>
          </select>
          <select
            className="reader-select"
            value={prefs.theme}
            onChange={(e) =>
              setPrefs((p) => ({ ...p, theme: e.target.value as ReaderTheme }))
            }
            title="Тема"
          >
            <option value="light">Светлая</option>
            <option value="sepia">Сепия</option>
            <option value="dark">Тёмная</option>
          </select>
        </div>
      </div>
      <div className={`reader-body${tocOpen ? "" : " no-toc"}`}>
        {tocOpen && (
          <aside className={`reader-toc ${fontClass}`}>
            {data && data.toc.length > 0 ? (
              <TocList
                entries={data.toc}
                activeId={activeChapter}
                onPick={(chapterId, anchor) => jumpTo(chapterId, anchor)}
              />
            ) : data ? (
              <ChapterList
                chapters={data.chapters}
                activeId={activeChapter}
                onPick={(id) => jumpTo(id)}
              />
            ) : null}
          </aside>
        )}
        <div
          className={`reader-scroll ${fontClass}`}
          ref={scrollRef}
          onScroll={onScroll}
          style={{ fontSize: `${prefs.fontSize}px` }}
        >
          {loading && <div className="reader-loading">Открываю книгу…</div>}
          {error && <div className="reader-error">{error}</div>}
          {data && (
            <div className="reader-content">
              {data.coverDataUrl && currentIdx <= 0 && (
                <img className="reader-cover" src={data.coverDataUrl} alt="cover" />
              )}
              {data.chapters.map((ch) => (
                <article
                  key={ch.id}
                  id={`reader-${ch.id}`}
                  className="reader-chapter"
                  ref={(el) => {
                    if (el) chapterRefs.current.set(ch.id, el);
                    else chapterRefs.current.delete(ch.id);
                  }}
                  dangerouslySetInnerHTML={{ __html: ch.html }}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function TocList({
  entries,
  activeId,
  onPick,
  depth = 0,
}: {
  entries: TocEntry[];
  activeId: string | null;
  onPick: (chapterId: string, anchor: string | null) => void;
  depth?: number;
}) {
  return (
    <ul className="reader-toc-list">
      {entries.map((e, i) => (
        <li key={`${e.chapterId}-${i}-${depth}`}>
          <button
            className={`reader-toc-item ${
              e.chapterId === activeId ? "active" : ""
            }`}
            style={{ paddingLeft: `${8 + depth * 14}px` }}
            disabled={!e.chapterId}
            onClick={() => onPick(e.chapterId, e.anchor)}
            title={e.title}
          >
            {e.title}
          </button>
          {e.children.length > 0 && (
            <TocList
              entries={e.children}
              activeId={activeId}
              onPick={onPick}
              depth={depth + 1}
            />
          )}
        </li>
      ))}
    </ul>
  );
}

function ChapterList({
  chapters,
  activeId,
  onPick,
}: {
  chapters: { id: string; title: string | null }[];
  activeId: string | null;
  onPick: (id: string) => void;
}) {
  return (
    <ul className="reader-toc-list">
      {chapters.map((c, i) => (
        <li key={c.id}>
          <button
            className={`reader-toc-item ${c.id === activeId ? "active" : ""}`}
            onClick={() => onPick(c.id)}
          >
            {c.title || `Глава ${i + 1}`}
          </button>
        </li>
      ))}
    </ul>
  );
}

function inEditable(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  if (!el) return false;
  const tag = el.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    el.isContentEditable === true
  );
}

function cssEscape(s: string): string {
  // Minimal CSS escape for selector use; webview has CSS.escape in modern WebKit.
  if (typeof (window as { CSS?: { escape?: (s: string) => string } }).CSS?.escape === "function") {
    return (window as unknown as { CSS: { escape: (s: string) => string } }).CSS.escape(s);
  }
  return s.replace(/([^a-zA-Z0-9_-])/g, "\\$1");
}
