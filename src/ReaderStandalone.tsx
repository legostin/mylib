import { useEffect, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { api } from "./lib/api";
import type { Book } from "./lib/types";
import { Reader } from "./components/Reader";

type Props = { bookId: number };

export function ReaderStandalone({ bookId }: Props) {
  const [book, setBook] = useState<Book | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    api
      .getBook(bookId)
      .then((b) => {
        if (cancelled) return;
        if (!b) {
          setError(`книга ${bookId} не найдена`);
        } else {
          setBook(b);
          const title = b.title || `Книга ${bookId}`;
          getCurrentWebviewWindow()
            .setTitle(title)
            .catch(() => {
              /* setting title is best-effort */
            });
        }
      })
      .catch((e) => !cancelled && setError(String(e)));
    return () => {
      cancelled = true;
    };
  }, [bookId]);

  const close = () => {
    getCurrentWebviewWindow()
      .close()
      .catch(() => {
        /* if closing fails, do nothing */
      });
  };

  if (error) {
    return (
      <div className="reader-standalone-error">
        {error}
        <button onClick={close}>Закрыть</button>
      </div>
    );
  }
  if (!book) {
    return <div className="reader-standalone-loading">Загрузка…</div>;
  }
  return <Reader book={book} onClose={close} />;
}
