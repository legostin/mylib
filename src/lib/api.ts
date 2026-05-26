import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  ArchiveHit,
  AuthorHit,
  AuthorView,
  Book,
  BookContent,
  BookFilters,
  BookListItem,
  CollectionInfo,
  ExportProgress,
  ExportSummary,
  GenreHit,
  ImportProgress,
  LanguageHit,
  LibraryStats,
  ListContents,
  ListKind,
  BookExternalMeta,
  ReaderBook,
  ReadingPosition,
  SearchResults,
  SearchScope,
  SeriesHit,
  ShareStatus,
  UserList,
} from "./types";

export const api = {
  stats: () => invoke<LibraryStats>("get_stats"),
  collectionInfo: () => invoke<CollectionInfo>("get_collection_info"),
  languages: (filters: BookFilters = {}) =>
    invoke<LanguageHit[]>("list_languages", { filters }),
  genres: (filters: BookFilters = {}) =>
    invoke<GenreHit[]>("list_genres", { filters }),
  archives: () => invoke<ArchiveHit[]>("list_archives"),
  lookupAuthorId: (display: string) =>
    invoke<number | null>("lookup_author_id", { display }),
  authorLetters: (filters: BookFilters) =>
    invoke<[string, number][]>("list_author_letters", { filters }),
  authorPrefixes: (letter: string, filters: BookFilters) =>
    invoke<[string, number][]>("list_author_prefixes", { letter, filters }),
  authorsByLetter: (letter: string, filters: BookFilters) =>
    invoke<AuthorHit[]>("list_authors_by_letter", { letter, filters }),
  seriesLetters: (filters: BookFilters) =>
    invoke<[string, number][]>("list_series_letters", { filters }),
  seriesByLetter: (letter: string, filters: BookFilters) =>
    invoke<SeriesHit[]>("list_series_by_letter", { letter, filters }),
  listBooks: (
    query: string | null,
    filters: BookFilters,
    limit = 500,
    offset = 0,
  ) =>
    invoke<BookListItem[]>("list_books", { query, filters, limit, offset }),
  getBook: (id: number) => invoke<Book | null>("get_book", { id }),
  importInpx: (path: string) => invoke<LibraryStats>("import_inpx", { path }),
  search: (
    query: string,
    scope: SearchScope,
    filters: BookFilters,
    limit = 30,
  ) =>
    invoke<SearchResults>("search", { query, scope, filters, limit }),
  getAuthorView: (id: number, filters: BookFilters) =>
    invoke<AuthorView>("get_author_view", { id, filters }),
  getSeriesView: (name: string, filters: BookFilters) =>
    invoke<BookListItem[]>("get_series_view", { name, filters }),
  getBookContent: (id: number) =>
    invoke<BookContent>("get_book_content", { id }),
  getReaderBook: (id: number) => invoke<ReaderBook>("get_reader_book", { id }),
  saveReadingPosition: (libId: string, chapterId: string, scroll: number) =>
    invoke<void>("save_reading_position", { libId, chapterId, scroll }),
  getReadingPosition: (libId: string) =>
    invoke<ReadingPosition | null>("get_reading_position", { libId }),
  getBookExternalMeta: (id: number) =>
    invoke<BookExternalMeta>("get_book_external_meta", { id }),

  // Lists
  listLists: () => invoke<UserList[]>("list_lists"),
  createList: (name: string) => invoke<UserList>("create_list", { name }),
  renameList: (id: number, name: string) =>
    invoke<void>("rename_list", { id, name }),
  deleteList: (id: number) => invoke<void>("delete_list", { id }),
  addToList: (listId: number, kind: ListKind, refKey: string) =>
    invoke<void>("add_to_list", { listId, kind, refKey }),
  removeFromList: (listId: number, kind: ListKind, refKey: string) =>
    invoke<void>("remove_from_list", { listId, kind, refKey }),
  listsContaining: (kind: ListKind, refKey: string) =>
    invoke<number[]>("lists_containing", { kind, refKey }),
  getListContents: (id: number) =>
    invoke<ListContents>("get_list_contents", { id }),

  // Export
  exportBooks: (bookIds: number[], targetDir: string) =>
    invoke<ExportSummary>("export_books", { bookIds, targetDir }),

  // Share / OPDS + ngrok
  shareStart: (opts?: { domain?: string | null; pooling?: boolean }) =>
    invoke<ShareStatus>("share_start", {
      domain: opts?.domain ?? null,
      pooling: opts?.pooling ?? false,
    }),
  shareStop: () => invoke<ShareStatus>("share_stop"),
  shareStatus: () => invoke<ShareStatus>("share_status"),
  shareKillStray: () => invoke<number>("share_kill_stray"),
  shareListDomains: () => invoke<string[]>("share_list_domains"),
};

export async function pickInpx(): Promise<string | null> {
  const result = await open({
    multiple: false,
    directory: false,
    filters: [{ name: "INPX library", extensions: ["inpx"] }],
  });
  if (typeof result === "string") return result;
  return null;
}

export async function pickFolder(): Promise<string | null> {
  const result = await open({ multiple: false, directory: true });
  if (typeof result === "string") return result;
  return null;
}

export function onImportProgress(
  cb: (p: ImportProgress) => void,
): Promise<UnlistenFn> {
  return listen<ImportProgress>("import-progress", (e) => cb(e.payload));
}

export function onExportProgress(
  cb: (p: ExportProgress) => void,
): Promise<UnlistenFn> {
  return listen<ExportProgress>("export-progress", (e) => cb(e.payload));
}

export function onShareStatus(
  cb: (s: ShareStatus) => void,
): Promise<UnlistenFn> {
  return listen<ShareStatus>("share-status", (e) => cb(e.payload));
}

export function onBookMetaUpdated(
  cb: (m: BookExternalMeta) => void,
): Promise<UnlistenFn> {
  return listen<BookExternalMeta>("book-meta-updated", (e) => cb(e.payload));
}
