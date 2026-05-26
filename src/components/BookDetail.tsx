import { useEffect, useState } from "react";
import { api, onBookMetaUpdated } from "../lib/api";
import { genreLabel } from "../lib/genresRu";
import type {
  Book,
  BookContent,
  BookExternalMeta,
  UserList,
} from "../lib/types";
import { AddToListMenu } from "./AddToListMenu";
import { ExportButton } from "./ExportButton";

type Props = {
  book: Book | null;
  lists: UserList[];
  onCreateList: (name: string) => Promise<UserList | null>;
  onListsChanged: () => void;
  onExport: (bookIds: number[]) => void | Promise<void>;
  onRead?: (book: Book) => void;
  onPickAuthor?: (id: number) => void;
  onPickSeries?: (name: string) => void;
  onPickGenre?: (code: string) => void;
};

export function BookDetail({
  book,
  lists,
  onCreateList,
  onListsChanged,
  onExport,
  onRead,
  onPickAuthor,
  onPickSeries,
  onPickGenre,
}: Props) {
  const [content, setContent] = useState<BookContent | null>(null);
  const [loadingContent, setLoadingContent] = useState(false);
  const [contentError, setContentError] = useState<string | null>(null);
  const [externalMeta, setExternalMeta] = useState<BookExternalMeta | null>(null);

  useEffect(() => {
    setContent(null);
    setContentError(null);
    setExternalMeta(null);
    if (!book) return;
    let cancelled = false;
    setLoadingContent(true);
    api
      .getBookContent(book.id)
      .then((c) => {
        if (cancelled) return;
        setContent(c);
      })
      .catch((e) => {
        if (cancelled) return;
        setContentError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoadingContent(false);
      });
    return () => {
      cancelled = true;
    };
  }, [book?.id]);

  // External metadata (Google Books / OpenLibrary): non-critical, fetched in
  // the background. The first response may already have cached entries; if
  // the backend kicks off fresh fetches we'll get follow-up `book-meta-updated`
  // events to merge in.
  useEffect(() => {
    setExternalMeta(null);
    if (!book || !book.lib_id) return;
    let cancelled = false;
    const expectedLibId = book.lib_id;
    api
      .getBookExternalMeta(book.id)
      .then((m) => {
        if (cancelled) return;
        setExternalMeta(m);
      })
      .catch(() => {
        /* network/cache miss is non-fatal; UI just won't show extras */
      });
    let unlisten: (() => void) | null = null;
    void onBookMetaUpdated((m) => {
      if (cancelled) return;
      if (m.libId !== expectedLibId) return;
      setExternalMeta(m);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [book?.id, book?.lib_id]);

  if (!book) {
    return (
      <section className="book-detail empty">
        <div className="empty-message">Выберите книгу слева</div>
      </section>
    );
  }
  const sizeKb = Math.max(1, Math.round(book.size / 1024));
  const ext = (book.ext || "").toLowerCase();
  const canRead = ext === "fb2" || ext === "epub";

  const authorEntries = book.authors.map((a) => ({
    display: [a.last, a.first, a.middle].filter(Boolean).join(" "),
  }));

  const onAuthorClick = async (display: string) => {
    if (!onPickAuthor || !display) return;
    try {
      const id = await api.lookupAuthorId(display);
      if (id != null) onPickAuthor(id);
    } catch {
      /* swallow — clicking author is a navigation nicety, not critical */
    }
  };

  return (
    <section className="book-detail">
      <div className="section-label">Выбрано</div>
      <h2 className="book-title">{book.title || "(без названия)"}</h2>
      {authorEntries.length > 0 && (
        <div className="authors">
          {authorEntries.map((a, i) => (
            <span key={`${a.display}-${i}`}>
              {i > 0 && ", "}
              {onPickAuthor ? (
                <button
                  className="link"
                  onClick={() => void onAuthorClick(a.display)}
                  title="Открыть страницу автора"
                >
                  {a.display}
                </button>
              ) : (
                a.display
              )}
            </span>
          ))}
        </div>
      )}

      <div className="detail-actions">
        {canRead && onRead && (
          <button
            className="primary read-btn"
            onClick={() => onRead(book)}
            title="Открыть в ридере"
          >
            ▶ Читать
          </button>
        )}
        <ExportButton onExport={() => onExport([book.id])} />
        <AddToListMenu
          kind="book"
          refKey={book.lib_id || null}
          lists={lists}
          onCreateList={onCreateList}
          onChanged={onListsChanged}
        />
      </div>

      {content?.coverDataUrl && (
        <img className="cover" src={content.coverDataUrl} alt="cover" />
      )}

      <dl className="meta-grid">
        {book.series && (
          <>
            <dt>Серия</dt>
            <dd>
              {onPickSeries ? (
                <button
                  className="link"
                  onClick={() => onPickSeries(book.series!)}
                  title="Открыть серию"
                >
                  {book.series}
                </button>
              ) : (
                book.series
              )}
              {book.ser_no ? ` #${book.ser_no}` : ""}
            </dd>
          </>
        )}
        {book.genres.length > 0 && (
          <>
            <dt>Жанры</dt>
            <dd title={book.genres.join(", ")}>
              {book.genres.map((code, i) => (
                <span key={code}>
                  {i > 0 && ", "}
                  {onPickGenre ? (
                    <button
                      className="link"
                      onClick={() => onPickGenre(code)}
                      title={`Фильтр по жанру: ${code}`}
                    >
                      {genreLabel(code)}
                    </button>
                  ) : (
                    genreLabel(code)
                  )}
                </span>
              ))}
            </dd>
          </>
        )}
        {book.lang && (
          <>
            <dt>Язык</dt>
            <dd>{book.lang}</dd>
          </>
        )}
        {book.date && (
          <>
            <dt>Дата</dt>
            <dd>{book.date}</dd>
          </>
        )}
        <dt>Формат</dt>
        <dd>
          {book.ext.toUpperCase()} · {sizeKb.toLocaleString("ru")} КБ
        </dd>
        <dt>Архив</dt>
        <dd className="mono">{book.archive}</dd>
        <dt>Файл</dt>
        <dd className="mono">
          {book.file}.{book.ext}
        </dd>
      </dl>

      <ExternalMetaPanel meta={externalMeta} />

      {(loadingContent ||
        contentError ||
        content?.description) && (
        <section className="detail-section">
          <div className="section-label">Аннотация</div>
          {loadingContent && (
            <div className="content-loading">Читаю FB2…</div>
          )}
          {contentError && (
            <div className="content-error">{contentError}</div>
          )}
          {content?.description && (
            <div className="description">
              {content.description.split(/\n+/).map((p, i) => (
                <p key={i}>{p}</p>
              ))}
            </div>
          )}
        </section>
      )}
    </section>
  );
}

const SOURCE_LABELS: Record<string, string> = {
  google: "Google Books",
  openlibrary: "OpenLibrary",
};

function ExternalMetaPanel({ meta }: { meta: BookExternalMeta | null }) {
  if (!meta) return null;
  // Pick the longest description across sources — both APIs vary in
  // verbosity, so the longer one is usually the more useful preview.
  const okEntries = meta.entries.filter((e) => e.status === "ok");
  const descEntry = okEntries
    .filter((e) => e.description && e.description.length > 0)
    .sort(
      (a, b) => (b.description?.length ?? 0) - (a.description?.length ?? 0),
    )[0];
  const ratings = okEntries.filter((e) => e.rating != null);
  if (!descEntry && ratings.length === 0 && !meta.fetching) return null;

  return (
    <div className="external-meta">
      <div className="external-meta-head">
        <span className="external-meta-title">Из открытых источников</span>
        {meta.fetching && <span className="external-meta-spinner" />}
      </div>
      {ratings.length > 0 && (
        <ul className="external-ratings">
          {ratings.map((e) => (
            <li key={e.source}>
              <span className="rating-source">
                {SOURCE_LABELS[e.source] ?? e.source}
              </span>
              <span className="rating-stars">
                ★ {e.rating?.toFixed(1)}
                {e.ratingCount != null && e.ratingCount > 0 && (
                  <span className="rating-count">
                    {" · "}
                    {e.ratingCount.toLocaleString("ru")}
                  </span>
                )}
              </span>
              {e.url && (
                <a
                  className="external-link"
                  href={e.url}
                  target="_blank"
                  rel="noreferrer"
                >
                  открыть →
                </a>
              )}
            </li>
          ))}
        </ul>
      )}
      {descEntry?.description && (
        <div className="external-description">
          <p>{descEntry.description}</p>
          <div className="external-source">
            источник: {SOURCE_LABELS[descEntry.source] ?? descEntry.source}
            {descEntry.url && (
              <>
                {" · "}
                <a href={descEntry.url} target="_blank" rel="noreferrer">
                  перейти
                </a>
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
