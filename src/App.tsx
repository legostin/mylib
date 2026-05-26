import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  api,
  onExportProgress,
  onImportProgress,
  pickFolder,
  pickInpx,
} from "./lib/api";
import type {
  AuthorView,
  Book,
  BookFilters,
  BookListItem,
  CollectionInfo,
  ExportProgress,
  ExportSummary,
  ImportProgress,
  LanguageHit,
  LibraryStats,
  ListContents,
  SearchResults,
  SearchScope,
  UserList,
} from "./lib/types";
import { Sidebar } from "./components/Sidebar";
import { BookDetail } from "./components/BookDetail";
import { Breadcrumbs } from "./components/Breadcrumbs";
import { AlphabetIndex } from "./components/AlphabetIndex";
import { SeriesAlphabet } from "./components/SeriesAlphabet";
import { GenresBrowse } from "./components/GenresBrowse";
import { LanguagesBrowse } from "./components/LanguagesBrowse";
import { LibraryHome } from "./components/LibraryHome";
import { ActiveFilterChips } from "./components/ActiveFilterChips";
import { AboutDialog } from "./components/AboutDialog";
import { CenterLoader } from "./components/CenterLoader";
import { invalidateAll as invalidateCache } from "./lib/cache";
import type { SidebarSection } from "./components/Sidebar";

const APP_VERSION = "0.2.3";
import { FilterPanel, countActiveFilters } from "./components/FilterPanel";
import { openReaderWindow } from "./lib/readerWindow";
import { ImportOverlay } from "./components/ImportOverlay";
import { ExportOverlay } from "./components/ExportOverlay";
import { SearchResultsView } from "./components/SearchResults";
import { AuthorPage } from "./components/AuthorPage";
import { SeriesPage } from "./components/SeriesPage";
import { ListPage } from "./components/ListPage";
import { SelectionBar } from "./components/SelectionBar";
import { SelectionDrawer } from "./components/SelectionDrawer";
import {
  emptySelection,
  isSelectionEmpty,
  type SelectedBook,
  type Selection,
} from "./lib/selection";

type Crumb =
  | { kind: "browse"; section: SidebarSection }
  | { kind: "search"; query: string; results: SearchResults }
  | { kind: "author"; id: number; display: string; data: AuthorView }
  | { kind: "series"; name: string; books: BookListItem[] }
  | { kind: "list"; id: number; name: string; data: ListContents };

function crumbLabel(c: Crumb): string {
  switch (c.kind) {
    case "browse":
      return BROWSE_LABELS[c.section];
    case "search":
      return `Поиск «${c.query}»`;
    case "author":
      return c.display;
    case "series":
      return c.name;
    case "list":
      return c.name;
  }
}

const BROWSE_LABELS: Record<SidebarSection, string> = {
  library: "Библиотека",
  authors: "Авторы",
  series: "Серии",
  genres: "Жанры",
  languages: "Языки",
};

function App() {
  const [stats, setStats] = useState<LibraryStats | null>(null);
  const [collection, setCollection] = useState<CollectionInfo | null>(null);
  const [languages, setLanguages] = useState<LanguageHit[]>([]);
  const [lists, setLists] = useState<UserList[]>([]);

  const [query, setQuery] = useState("");
  const [scope, setScope] = useState<SearchScope>("all");
  const [filters, setFilters] = useState<BookFilters>(() => loadFilters());
  const [filterAuthorLabel, setFilterAuthorLabel] = useState<string | null>(
    null,
  );
  const [filtersOpen, setFiltersOpen] = useState(false);

  const [trail, setTrail] = useState<Crumb[]>([
    { kind: "browse", section: "library" },
  ]);
  const current = trail[trail.length - 1];
  const [activeSection, setActiveSection] = useState<SidebarSection>("library");
  const [aboutOpen, setAboutOpen] = useState(false);
  const [searching, setSearching] = useState(false);

  const [selectedBook, setSelectedBook] = useState<Book | null>(null);
  const [selection, setSelection] = useState<Selection>(emptySelection);
  const [importing, setImporting] = useState<ImportProgress | null>(null);
  const [exporting, setExporting] = useState<ExportProgress | null>(null);
  const [exportSummary, setExportSummary] = useState<ExportSummary | null>(
    null,
  );
  const [error, setError] = useState<string | null>(null);

  const refreshLists = useCallback(async () => {
    try {
      const l = await api.listLists();
      setLists(l);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const refresh = useCallback(async () => {
    try {
      const [s, c, l, ls] = await Promise.all([
        api.stats(),
        api.collectionInfo(),
        api.languages(),
        api.listLists(),
      ]);
      setStats(s);
      setCollection(c);
      setLanguages(l);
      setLists(ls);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    void refresh();
    let unlistenImport: (() => void) | null = null;
    let unlistenExport: (() => void) | null = null;
    void onImportProgress((p) => {
      setImporting(p);
      if (p.stage === "done") {
        // Wipe the SWR cache — every list/count is recomputed off the new
        // SQLite rows now.
        invalidateCache();
        window.setTimeout(() => setImporting(null), 700);
        void refresh();
      }
    }).then((fn) => (unlistenImport = fn));
    void onExportProgress((p) => {
      setExporting(p);
    }).then((fn) => (unlistenExport = fn));
    return () => {
      if (unlistenImport) unlistenImport();
      if (unlistenExport) unlistenExport();
    };
  }, [refresh]);

  useEffect(() => {
    saveFilters(filters);
  }, [filters]);

  // Resolve the selected author filter to a display name (for the chip /
  // breadcrumb label). Cached so we don't refetch each render.
  useEffect(() => {
    const id = filters.authorId;
    if (id == null) {
      setFilterAuthorLabel(null);
      return;
    }
    let cancelled = false;
    api
      .getAuthorView(id, { ...filters, authorId: null })
      .then((v) => !cancelled && setFilterAuthorLabel(v.display))
      .catch(() => !cancelled && setFilterAuthorLabel(null));
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filters.authorId]);

  const debouncedQuery = useDebounced(query, 220);
  useEffect(() => {
    let cancelled = false;
    const q = debouncedQuery.trim();
    if (!q) {
      setSearching(false);
      // Clearing search pops the trail back to root if a search is on top.
      setTrail((t) =>
        t.length > 0 && t[t.length - 1].kind === "search" ? t.slice(0, -1) : t,
      );
      return;
    }
    setSearching(true);
    api
      .search(q, scope, filters, 30)
      .then((r) => {
        if (cancelled) return;
        // Typing a global search resets the trail to [browse, search]. Any
        // prior drill-down is forgotten — the user is explicitly searching
        // again.
        setTrail([
          { kind: "browse", section: activeSection },
          { kind: "search", query: q, results: r },
        ]);
      })
      .catch((e) => !cancelled && setError(String(e)))
      .finally(() => {
        if (!cancelled) setSearching(false);
      });
    return () => {
      // Marking cancelled drops the result of an in-flight request — the
      // backend SQL keeps running, but we no longer race to update state.
      cancelled = true;
    };
  }, [debouncedQuery, scope, filters]);

  useEffect(() => {
    let cancelled = false;
    if (current.kind === "author") {
      const id = current.id;
      api
        .getAuthorView(id, filters)
        .then((data) => {
          if (cancelled) return;
          setTrail((t) => {
            const next = t.slice();
            const last = next[next.length - 1];
            if (last && last.kind === "author" && last.id === id) {
              next[next.length - 1] = { ...last, data };
            }
            return next;
          });
        })
        .catch((e) => !cancelled && setError(String(e)));
    } else if (current.kind === "series") {
      const name = current.name;
      api
        .getSeriesView(name, filters)
        .then((books) => {
          if (cancelled) return;
          setTrail((t) => {
            const next = t.slice();
            const last = next[next.length - 1];
            if (last && last.kind === "series" && last.name === name) {
              next[next.length - 1] = { ...last, books };
            }
            return next;
          });
        })
        .catch((e) => !cancelled && setError(String(e)));
    }
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filters]);

  const onPick = useCallback(async () => {
    setError(null);
    const path = await pickInpx();
    if (!path) return;
    setImporting({ stage: "reading", bytesDone: 0, bytesTotal: 0, records: 0 });
    try {
      await api.importInpx(path);
      await refresh();
    } catch (e) {
      setError(String(e));
      setImporting(null);
    }
  }, [refresh]);

  const onPickBook = useCallback(async (id: number) => {
    try {
      const book = await api.getBook(id);
      setSelectedBook(book);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const onPickAuthor = useCallback(
    async (id: number) => {
      setError(null);
      try {
        const data = await api.getAuthorView(id, filters);
        setTrail((t) => [
          ...t,
          { kind: "author", id, display: data.display, data },
        ]);
        setQuery("");
      } catch (e) {
        setError(String(e));
      }
    },
    [filters],
  );

  const onPickSeries = useCallback(
    async (name: string) => {
      setError(null);
      try {
        const books = await api.getSeriesView(name, filters);
        setTrail((t) => [...t, { kind: "series", name, books }]);
        setQuery("");
      } catch (e) {
        setError(String(e));
      }
    },
    [filters],
  );

  const onPickList = useCallback(async (id: number) => {
    setError(null);
    try {
      const data = await api.getListContents(id);
      // Lists are sidebar-rooted, so opening one resets the trail rather than
      // appending to whatever the user was drilling into.
      setTrail([
        { kind: "browse", section: "library" },
        { kind: "list", id, name: data.list.name, data },
      ]);
      setQuery("");
    } catch (e) {
      setError(String(e));
    }
  }, []);

  const onCrumbClick = useCallback((index: number) => {
    setTrail((t) => t.slice(0, index + 1));
    setQuery("");
  }, []);

  const onCreateList = useCallback(
    async (name: string): Promise<UserList | null> => {
      setError(null);
      try {
        const created = await api.createList(name);
        await refreshLists();
        return created;
      } catch (e) {
        setError(String(e));
        return null;
      }
    },
    [refreshLists],
  );

  const onDeleteList = useCallback(
    async (id: number) => {
      setError(null);
      try {
        await api.deleteList(id);
        await refreshLists();
        setTrail((t) => {
          // Drop the deleted list from the trail wherever it appears.
          const pruned = t.filter((c) => !(c.kind === "list" && c.id === id));
          return pruned.length > 0
            ? pruned
            : [{ kind: "browse", section: "library" }];
        });
      } catch (e) {
        setError(String(e));
      }
    },
    [refreshLists],
  );

  const onListsChanged = useCallback(async () => {
    await refreshLists();
    if (current.kind === "list") {
      const listId = current.id;
      try {
        const data = await api.getListContents(listId);
        setTrail((t) => {
          const next = t.slice();
          const last = next[next.length - 1];
          if (last && last.kind === "list" && last.id === listId) {
            next[next.length - 1] = { ...last, name: data.list.name, data };
          }
          return next;
        });
      } catch (e) {
        setError(String(e));
      }
    }
  }, [refreshLists, current]);

  const onRemoveOrphan = useCallback(
    async (kind: "book" | "author" | "series", refKey: string) => {
      if (current.kind !== "list") return;
      const listId = current.id;
      try {
        await api.removeFromList(listId, kind, refKey);
        const data = await api.getListContents(listId);
        setTrail((t) => {
          const next = t.slice();
          const last = next[next.length - 1];
          if (last && last.kind === "list" && last.id === listId) {
            next[next.length - 1] = { ...last, name: data.list.name, data };
          }
          return next;
        });
        void refreshLists();
      } catch (e) {
        setError(String(e));
      }
    },
    [current, refreshLists],
  );

  const toggleBook = useCallback((b: SelectedBook) => {
    setSelection((s) => {
      const books = new Map(s.books);
      if (books.has(b.id)) books.delete(b.id);
      else books.set(b.id, b);
      return { ...s, books };
    });
  }, []);

  const toggleSeries = useCallback((name: string) => {
    setSelection((s) => {
      const series = new Set(s.series);
      if (series.has(name)) series.delete(name);
      else series.add(name);
      return { ...s, series };
    });
  }, []);

  const toggleAuthor = useCallback((a: { id: number; display: string }) => {
    setSelection((s) => {
      const authors = new Map(s.authors);
      if (authors.has(a.id)) authors.delete(a.id);
      else authors.set(a.id, a);
      return { ...s, authors };
    });
  }, []);

  const clearSelection = useCallback(() => {
    setSelection(emptySelection());
  }, []);

  const removeBookFromSelection = useCallback((id: number) => {
    setSelection((s) => {
      if (!s.books.has(id)) return s;
      const books = new Map(s.books);
      books.delete(id);
      return { ...s, books };
    });
  }, []);

  const removeSeriesFromSelection = useCallback((name: string) => {
    setSelection((s) => {
      if (!s.series.has(name)) return s;
      const series = new Set(s.series);
      series.delete(name);
      return { ...s, series };
    });
  }, []);

  const removeAuthorFromSelection = useCallback((id: number) => {
    setSelection((s) => {
      if (!s.authors.has(id)) return s;
      const authors = new Map(s.authors);
      authors.delete(id);
      return { ...s, authors };
    });
  }, []);

  const [drawerOpen, setDrawerOpen] = useState(false);

  const bulkAddToList = useCallback(
    async (listId: number) => {
      setError(null);
      try {
        for (const b of selection.books.values()) {
          if (b.libId) await api.addToList(listId, "book", b.libId);
        }
        for (const name of selection.series) {
          await api.addToList(listId, "series", name);
        }
        for (const a of selection.authors.values()) {
          // Author lists are keyed by display name, matching the convention
          // already used elsewhere (lists_containing, list_items).
          await api.addToList(listId, "author", a.display);
        }
        await refreshLists();
      } catch (e) {
        setError(String(e));
      }
    },
    [selection, refreshLists],
  );

  const doExport = useCallback(async (bookIds: number[]) => {
    setError(null);
    if (bookIds.length === 0) {
      setError("Нет книг для экспорта");
      return;
    }
    const dir = await pickFolder();
    if (!dir) return;
    setExportSummary(null);
    setExporting({
      stage: "starting",
      done: 0,
      total: bookIds.length,
      current: "",
    });
    try {
      const summary = await api.exportBooks(bookIds, dir);
      setExporting(null);
      setExportSummary(summary);
    } catch (e) {
      setExporting(null);
      setError(String(e));
    }
  }, []);

  const exportSelection = useCallback(async () => {
    setError(null);
    const ids = new Set<number>(selection.books.keys());
    try {
      // Series are stored by name; resolve to current book ids on the fly so
      // we don't have to keep the lists in sync if the user selects across
      // multiple pages.
      for (const name of selection.series) {
        const rows = await api.getSeriesView(name, filters);
        for (const r of rows) ids.add(r.id);
      }
      for (const a of selection.authors.values()) {
        const view = await api.getAuthorView(a.id, filters);
        for (const g of view.groups) {
          for (const b of g.books) ids.add(b.id);
        }
      }
    } catch (e) {
      setError(String(e));
      return;
    }
    if (ids.size === 0) {
      setError("Выбор пуст");
      return;
    }
    await doExport([...ids]);
    clearSelection();
  }, [selection, filters, doExport, clearSelection]);

  const dismissExport = useCallback(() => {
    setExportSummary(null);
    setExporting(null);
  }, []);

  const headerStatus = useMemo(() => {
    if (importing) return null;
    if (!stats) return "";
    return `${stats.books.toLocaleString("ru")} книг · ${stats.authors.toLocaleString("ru")} авторов · ${stats.series.toLocaleString("ru")} серий`;
  }, [importing, stats]);

  const selectedId = selectedBook?.id ?? null;
  const activeListId = current.kind === "list" ? current.id : null;
  const showBreadcrumbs = trail.length > 1;

  return (
    <div className="app">
      <Sidebar
        collection={collection}
        stats={stats}
        lists={lists}
        activeListId={activeListId}
        activeSection={activeSection}
        onSelectSection={(section) => {
          setActiveSection(section);
          setTrail([{ kind: "browse", section }]);
          setQuery("");
        }}
        onSelectList={onPickList}
        onCreateList={onCreateList}
        onDeleteList={onDeleteList}
        onOpen={onPick}
        onAbout={() => setAboutOpen(true)}
        busy={!!importing}
      />
      <main className="main">
        <header className="topbar">
          <div className="search-row">
            <button
              className="nav-back"
              onClick={() => onCrumbClick(Math.max(0, trail.length - 2))}
              disabled={trail.length <= 1 || !!importing}
              title="Назад"
              aria-label="Назад"
            >
              ‹
            </button>
            <input
              type="search"
              placeholder="Поиск авторов, серий, книг…"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              disabled={!!importing}
            />
            <div className="filter-button-wrap">
              <button
                className={`filter-button${
                  countActiveFilters(filters) > 0 ? " active" : ""
                }`}
                onClick={() => setFiltersOpen((v) => !v)}
                disabled={!!importing}
                title="Фильтры"
              >
                Фильтры
                {countActiveFilters(filters) > 0 && (
                  <span className="filter-button-badge">
                    {countActiveFilters(filters)}
                  </span>
                )}
              </button>
              <FilterPanel
                open={filtersOpen}
                onClose={() => setFiltersOpen(false)}
                scope={scope}
                onScopeChange={setScope}
                filters={filters}
                onChange={setFilters}
                languages={languages}
                currentAuthorLabel={filterAuthorLabel}
              />
            </div>
          </div>
          <ActiveFilterChips
            filters={filters}
            authorLabel={filterAuthorLabel}
            onChange={setFilters}
            onOpenPanel={() => setFiltersOpen(true)}
          />
          <div className="status">{headerStatus}</div>
        </header>
        {importing && <ImportOverlay progress={importing} />}
        <ExportOverlay
          progress={exporting}
          summary={exportSummary}
          onDismiss={dismissExport}
        />
        <div className="main-body">
          <div className="center">
            {showBreadcrumbs && (
              <Breadcrumbs
                items={trail.map((c, i) => ({
                  label: crumbLabel(c),
                  onClick:
                    i === trail.length - 1 ? undefined : () => onCrumbClick(i),
                }))}
              />
            )}
            {searching && (
              <CenterLoader
                label="Ищу…"
                hint={query.trim() ? `«${query.trim()}»` : null}
              />
            )}
            {current.kind === "browse" && current.section === "library" && (
              <LibraryHome
                stats={stats}
                collection={collection}
                onPickSection={(section) => {
                  setActiveSection(section);
                  setTrail([{ kind: "browse", section }]);
                }}
              />
            )}
            {current.kind === "browse" && current.section === "authors" && (
              <AlphabetIndex
                filters={filters}
                onPickAuthor={onPickAuthor}
                selection={selection}
                onToggleAuthor={toggleAuthor}
              />
            )}
            {current.kind === "browse" && current.section === "series" && (
              <SeriesAlphabet
                filters={filters}
                onPickSeries={onPickSeries}
              />
            )}
            {current.kind === "browse" && current.section === "genres" && (
              <GenresBrowse
                filters={filters}
                onPickGenre={(code) => {
                  setFilters((f) => ({ ...f, genre: code }));
                  setActiveSection("authors");
                  setTrail([{ kind: "browse", section: "authors" }]);
                }}
              />
            )}
            {current.kind === "browse" && current.section === "languages" && (
              <LanguagesBrowse
                filters={filters}
                onPickLang={(code) => {
                  setFilters((f) => ({ ...f, lang: code }));
                  setActiveSection("authors");
                  setTrail([{ kind: "browse", section: "authors" }]);
                }}
              />
            )}
            {current.kind === "search" && (
              <SearchResultsView
                results={current.results}
                selectedBookId={selectedId}
                onPickAuthor={onPickAuthor}
                onPickSeries={onPickSeries}
                onPickBook={onPickBook}
                selection={selection}
                onToggleBook={toggleBook}
                onToggleSeries={toggleSeries}
              />
            )}
            {current.kind === "author" && (
              <AuthorPage
                author={current.data}
                selectedBookId={selectedId}
                lists={lists}
                onCreateList={onCreateList}
                onListsChanged={onListsChanged}
                onPickBook={onPickBook}
                onPickSeries={onPickSeries}
                onExportAll={async () => {
                  if (current.kind !== "author") return;
                  const ids = current.data.groups.flatMap((g) =>
                    g.books.map((b) => b.id),
                  );
                  await doExport(ids);
                }}
                selection={selection}
                onToggleBook={toggleBook}
                onToggleSeries={toggleSeries}
              />
            )}
            {current.kind === "series" && (
              <SeriesPage
                name={current.name}
                books={current.books}
                selectedBookId={selectedId}
                lists={lists}
                onCreateList={onCreateList}
                onListsChanged={onListsChanged}
                onPickBook={onPickBook}
                selection={selection}
                onToggleBook={toggleBook}
              />
            )}
            {current.kind === "list" && (
              <ListPage
                contents={current.data}
                selectedBookId={selectedId}
                onPickBook={onPickBook}
                onPickAuthor={onPickAuthor}
                onPickSeries={onPickSeries}
                onRemoveOrphan={onRemoveOrphan}
                selection={selection}
                onToggleBook={toggleBook}
                onToggleSeries={toggleSeries}
              />
            )}
            {!isSelectionEmpty(selection) && (
              <SelectionBar
                selection={selection}
                lists={lists}
                onCreateList={onCreateList}
                onBulkAddToList={bulkAddToList}
                onShow={() => setDrawerOpen(true)}
                onExport={exportSelection}
                onClear={clearSelection}
              />
            )}
            {drawerOpen && (
              <SelectionDrawer
                selection={selection}
                lists={lists}
                onCreateList={onCreateList}
                onBulkAddToList={bulkAddToList}
                onRemoveBook={removeBookFromSelection}
                onRemoveSeries={removeSeriesFromSelection}
                onRemoveAuthor={removeAuthorFromSelection}
                onPickBook={onPickBook}
                onPickSeries={onPickSeries}
                onPickAuthor={onPickAuthor}
                onClear={clearSelection}
                onExport={exportSelection}
                onClose={() => setDrawerOpen(false)}
              />
            )}
          </div>
          <BookDetail
            book={selectedBook}
            lists={lists}
            onCreateList={onCreateList}
            onListsChanged={onListsChanged}
            onExport={doExport}
            onRead={(book) => {
              openReaderWindow(book).catch((e) => setError(String(e)));
            }}
            onPickAuthor={onPickAuthor}
            onPickSeries={onPickSeries}
            onPickGenre={(code) => {
              setFilters((f) => ({ ...f, genre: code }));
              setActiveSection("authors");
              setTrail([{ kind: "browse", section: "authors" }]);
            }}
          />
        </div>
        {error && (
          <div className="error" onClick={() => setError(null)} title="Закрыть">
            {error}
          </div>
        )}
      </main>
      <AboutDialog
        open={aboutOpen}
        version={APP_VERSION}
        onClose={() => setAboutOpen(false)}
      />
    </div>
  );
}

const FILTERS_KEY = "mylib.filters";

function loadFilters(): BookFilters {
  try {
    const raw = localStorage.getItem(FILTERS_KEY);
    if (!raw) return {};
    const f = JSON.parse(raw) as BookFilters;
    return {
      lang: f.lang ?? null,
      genre: f.genre ?? null,
      archive: f.archive ?? null,
      authorId: f.authorId ?? null,
    };
  } catch {
    return {};
  }
}

function saveFilters(f: BookFilters) {
  try {
    localStorage.setItem(FILTERS_KEY, JSON.stringify(f));
  } catch {
    /* quota: ignore */
  }
}

function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  const t = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(() => {
    if (t.current) clearTimeout(t.current);
    t.current = setTimeout(() => setV(value), ms);
    return () => {
      if (t.current) clearTimeout(t.current);
    };
  }, [value, ms]);
  return v;
}

export default App;
