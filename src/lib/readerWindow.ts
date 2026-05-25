import {
  WebviewWindow,
  getAllWebviewWindows,
} from "@tauri-apps/api/webviewWindow";
import type { Book } from "./types";

/// Opens (or focuses, if already open) a dedicated reader window for the given
/// book. Each book gets its own window label so a single book can't be opened
/// twice in parallel.
export async function openReaderWindow(book: Book): Promise<void> {
  const label = `reader-${book.id}`;
  const existing = (await getAllWebviewWindows()).find((w) => w.label === label);
  if (existing) {
    await existing.unminimize().catch(() => {});
    await existing.setFocus().catch(() => {});
    return;
  }
  // Anchor hash drives main.tsx routing in the new webview.
  const url = `index.html#reader=${book.id}`;
  const win = new WebviewWindow(label, {
    url,
    title: book.title || `Книга ${book.id}`,
    width: 1000,
    height: 800,
    minWidth: 600,
    minHeight: 400,
    resizable: true,
    decorations: true,
  });
  await new Promise<void>((resolve, reject) => {
    win.once("tauri://created", () => resolve());
    win.once("tauri://error", (e) =>
      reject(new Error(`не удалось открыть окно ридера: ${String(e.payload)}`)),
    );
  });
}
