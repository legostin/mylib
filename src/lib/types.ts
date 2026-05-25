export type AuthorName = {
  last: string;
  first: string;
  middle: string;
};

export type BookListItem = {
  id: number;
  libId: string;
  title: string;
  authors: string;
  series: string | null;
  serNo: number | null;
  lang: string;
  size: number;
  ext: string;
};

export type Book = {
  id: number;
  title: string;
  authors: AuthorName[];
  genres: string[];
  series: string | null;
  ser_no: number | null;
  size: number;
  lang: string;
  librate: number | null;
  date: string;
  ext: string;
  file: string;
  archive: string;
  lib_id: string;
  deleted: boolean;
};

export type BookContent = {
  description: string;
  coverDataUrl: string | null;
  source: string;
};

export type LibraryStats = {
  books: number;
  authors: number;
  series: number;
};

export type CollectionInfo = {
  name: string;
  version: string;
  inpxPath: string;
  booksDir: string;
};

export type ImportProgress = {
  stage: "reading" | "indexing" | "done" | string;
  bytesDone: number;
  bytesTotal: number;
  records: number;
};

export type AuthorHit = {
  id: number;
  display: string;
  bookCount: number;
};

export type SeriesHit = {
  name: string;
  bookCount: number;
};

export type SearchScope = "all" | "authors" | "series" | "books";

export type SearchResults = {
  authors: AuthorHit[];
  series: SeriesHit[];
  books: BookListItem[];
};

export type SeriesGroup = {
  name: string | null;
  books: BookListItem[];
};

export type AuthorView = {
  id: number;
  display: string;
  groups: SeriesGroup[];
};

export type LanguageHit = {
  code: string;
  count: number;
};

export type GenreHit = {
  code: string;
  count: number;
};

export type ArchiveHit = {
  name: string;
  count: number;
};

/// All filters are independent of the current search query. Empty/null fields
/// mean "no constraint" — backend accepts a partial object.
export type BookFilters = {
  lang?: string | null;
  genre?: string | null;
  archive?: string | null;
  authorId?: number | null;
};

export type UserList = {
  id: number;
  name: string;
  builtin: boolean;
  itemCount: number;
};

export type OrphanItem = {
  kind: "book" | "author" | "series";
  refKey: string;
};

export type ListContents = {
  list: UserList;
  books: BookListItem[];
  authors: AuthorHit[];
  series: SeriesHit[];
  orphans: OrphanItem[];
};

export type ListKind = "book" | "author" | "series";

export type ExportProgress = {
  stage: "starting" | "copying" | "done" | string;
  done: number;
  total: number;
  current: string;
};

export type ExportError = {
  bookId: number;
  title: string;
  message: string;
};

export type ExportSummary = {
  total: number;
  copied: number;
  skipped: number;
  targetDir: string;
  errors: ExportError[];
};

/// Stable key used when storing the item in a user list.
export function bookRefKey(b: { libId: string }): string | null {
  return b.libId || null;
}

export type ReaderChapter = {
  id: string;
  title: string | null;
  html: string;
};

export type TocEntry = {
  title: string;
  chapterId: string;
  anchor: string | null;
  children: TocEntry[];
};

export type ReadingPosition = {
  libId: string;
  chapterId: string;
  scroll: number;
  updatedAt: number;
};

export type ReaderBook = {
  title: string;
  authors: string[];
  lang: string;
  coverDataUrl: string | null;
  chapters: ReaderChapter[];
  toc: TocEntry[];
  format: "fb2" | "epub" | string;
  position: ReadingPosition | null;
};

export type ExternalMetaEntry = {
  source: string;
  status: string;
  description: string | null;
  rating: number | null;
  ratingCount: number | null;
  url: string | null;
  fetchedAt: number;
};

export type BookExternalMeta = {
  libId: string;
  entries: ExternalMetaEntry[];
  fetching: boolean;
};

export type ShareStatus = {
  running: boolean;
  localUrl: string | null;
  publicUrl: string | null;
  error: string | null;
  /// "stopped" | "starting" | "running" | "error"
  stage: string;
};
