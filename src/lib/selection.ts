/// Compact preview of a selected book so the selection drawer can render
/// titles without re-fetching from the backend. Captured at toggle time.
export type SelectedBook = {
  id: number;
  libId: string;
  title: string;
  authors: string;
};

/// Snapshot of a selected author — id is the stable key, display kept for the
/// drawer / bulk-list UX without an extra fetch.
export type SelectedAuthor = {
  id: number;
  display: string;
};

/// App-level multi-selection. Each kind has its own keying:
/// - books by integer id,
/// - authors by integer id,
/// - series by name (the name *is* the catalog-level identity).
export type Selection = {
  books: Map<number, SelectedBook>;
  authors: Map<number, SelectedAuthor>;
  series: Set<string>;
};

export const emptySelection = (): Selection => ({
  books: new Map(),
  authors: new Map(),
  series: new Set(),
});

export const selectionSize = (s: Selection): number =>
  s.books.size + s.authors.size + s.series.size;

export const isSelectionEmpty = (s: Selection): boolean =>
  selectionSize(s) === 0;
