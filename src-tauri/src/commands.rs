use std::path::PathBuf;
use std::sync::Arc;

use tauri::{AppHandle, Runtime, State};

use crate::error::{Error, Result};
use crate::library::LibraryState;
use crate::external_meta::{fetch_google_books, fetch_openlibrary, now_secs};
use crate::model::{
    ArchiveHit, AuthorHit, AuthorView, Book, BookContent, BookExternalMeta, BookFilters,
    BookListItem, ExportSummary, GenreHit, LanguageHit, LibraryStats, ListContents, ReaderBook,
    ReadingPosition, SearchResults, SeriesHit, UserList,
};
use tauri::Emitter;
use crate::share::{ShareState, ShareStatus};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionInfo {
    pub name: String,
    pub version: String,
    pub inpx_path: String,
    pub books_dir: String,
}

/// Normalise an optional `BookFilters` payload coming from the JS side: drop
/// empty strings so they don't get turned into `WHERE x = ''` clauses.
fn norm_filters(f: Option<BookFilters>) -> BookFilters {
    let mut f = f.unwrap_or_default();
    f.lang = f.lang.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    f.genre = f.genre.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    f.archive = f.archive.and_then(|s| {
        let t = s.trim();
        if t.is_empty() {
            None
        } else {
            Some(t.to_string())
        }
    });
    f
}

#[tauri::command]
pub fn get_stats(state: State<Arc<LibraryState>>) -> Result<LibraryStats> {
    state.stats()
}

#[tauri::command]
pub fn get_collection_info(state: State<Arc<LibraryState>>) -> Result<CollectionInfo> {
    Ok(CollectionInfo {
        name: state.get_meta("collection_name")?.unwrap_or_default(),
        version: state.get_meta("collection_version")?.unwrap_or_default(),
        inpx_path: state.get_meta("inpx_path")?.unwrap_or_default(),
        books_dir: state.get_meta("books_dir")?.unwrap_or_default(),
    })
}

#[tauri::command]
pub fn list_languages(
    state: State<Arc<LibraryState>>,
    filters: Option<BookFilters>,
) -> Result<Vec<LanguageHit>> {
    let filters = norm_filters(filters);
    state.list_languages(&filters)
}

#[tauri::command]
pub fn list_genres(
    state: State<Arc<LibraryState>>,
    filters: Option<BookFilters>,
) -> Result<Vec<GenreHit>> {
    let filters = norm_filters(filters);
    state.list_genres(&filters)
}

#[tauri::command]
pub fn list_archives(state: State<Arc<LibraryState>>) -> Result<Vec<ArchiveHit>> {
    state.list_archives()
}

#[tauri::command]
pub fn list_books(
    state: State<Arc<LibraryState>>,
    query: Option<String>,
    filters: Option<BookFilters>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<BookListItem>> {
    let filters = norm_filters(filters);
    state.list_books(
        query.as_deref(),
        &filters,
        limit.unwrap_or(200),
        offset.unwrap_or(0),
    )
}

#[tauri::command]
pub fn get_book(state: State<Arc<LibraryState>>, id: i64) -> Result<Option<Book>> {
    state.get_book(id)
}

#[tauri::command]
pub fn search(
    state: State<Arc<LibraryState>>,
    query: String,
    scope: String,
    filters: Option<BookFilters>,
    limit: Option<i64>,
) -> Result<SearchResults> {
    let limit = limit.unwrap_or(30);
    let (authors, series, books) = match scope.as_str() {
        "authors" => (true, false, false),
        "series" => (false, true, false),
        "books" => (false, false, true),
        _ => (true, true, true),
    };
    let filters = norm_filters(filters);
    state.search(&query, authors, series, books, &filters, limit)
}

#[tauri::command]
pub fn get_author_view(
    state: State<Arc<LibraryState>>,
    id: i64,
    filters: Option<BookFilters>,
) -> Result<AuthorView> {
    let filters = norm_filters(filters);
    state.author_view(id, &filters)
}

#[tauri::command]
pub fn get_series_view(
    state: State<Arc<LibraryState>>,
    name: String,
    filters: Option<BookFilters>,
) -> Result<Vec<BookListItem>> {
    let filters = norm_filters(filters);
    state.series_view(&name, &filters)
}

#[tauri::command]
pub fn lookup_author_id(
    state: State<Arc<LibraryState>>,
    display: String,
) -> Result<Option<i64>> {
    state.author_id_by_display(&display)
}

#[tauri::command]
pub fn list_author_letters(
    state: State<Arc<LibraryState>>,
    filters: Option<BookFilters>,
) -> Result<Vec<(String, i64)>> {
    let filters = norm_filters(filters);
    state.author_first_letters(&filters)
}

#[tauri::command]
pub fn list_authors_by_letter(
    state: State<Arc<LibraryState>>,
    letter: String,
    filters: Option<BookFilters>,
) -> Result<Vec<AuthorHit>> {
    let filters = norm_filters(filters);
    state.authors_by_prefix(&filters, &letter)
}

#[tauri::command]
pub fn list_author_prefixes(
    state: State<Arc<LibraryState>>,
    letter: String,
    filters: Option<BookFilters>,
) -> Result<Vec<(String, i64)>> {
    let filters = norm_filters(filters);
    state.author_two_letter_prefixes(&filters, &letter)
}

#[tauri::command]
pub fn list_series_letters(
    state: State<Arc<LibraryState>>,
    filters: Option<BookFilters>,
) -> Result<Vec<(String, i64)>> {
    let filters = norm_filters(filters);
    state.series_first_letters(&filters)
}

#[tauri::command]
pub fn list_series_by_letter(
    state: State<Arc<LibraryState>>,
    letter: String,
    filters: Option<BookFilters>,
) -> Result<Vec<SeriesHit>> {
    let filters = norm_filters(filters);
    state.series_by_prefix(&filters, &letter)
}

#[tauri::command]
pub fn get_book_content(state: State<Arc<LibraryState>>, id: i64) -> Result<BookContent> {
    state.book_content(id)
}

#[tauri::command]
pub async fn get_reader_book(
    state: State<'_, Arc<LibraryState>>,
    id: i64,
) -> Result<ReaderBook> {
    let state = Arc::clone(state.inner());
    let join = tauri::async_runtime::spawn_blocking(move || state.reader_book(id));
    join.await
        .map_err(|e| Error::Other(format!("reader worker: {e}")))?
}

#[tauri::command]
pub fn save_reading_position(
    state: State<Arc<LibraryState>>,
    lib_id: String,
    chapter_id: String,
    scroll: f64,
) -> Result<()> {
    if lib_id.is_empty() {
        return Ok(());
    }
    state.save_reading_position(&lib_id, &chapter_id, scroll)
}

#[tauri::command]
pub fn get_reading_position(
    state: State<Arc<LibraryState>>,
    lib_id: String,
) -> Result<Option<ReadingPosition>> {
    if lib_id.is_empty() {
        return Ok(None);
    }
    state.reading_position(&lib_id)
}

/// TTL for cached external metadata. Past this, we silently re-fetch on the
/// next request so ratings stay reasonably fresh without hammering the APIs.
const EXTERNAL_META_TTL_SECS: i64 = 30 * 24 * 60 * 60;
const EXTERNAL_META_SOURCES: &[&str] = &["google", "openlibrary"];

/// Returns whatever metadata is currently in cache for this book. If any
/// sources are missing or stale, kicks off a background fetch and emits
/// `book-meta-updated` once it lands. The `fetching` flag on the returned
/// object lets the UI show a spinner while we wait.
#[tauri::command]
pub async fn get_book_external_meta<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<LibraryState>>,
    id: i64,
) -> Result<BookExternalMeta> {
    let state_arc = Arc::clone(state.inner());
    let (lib_id, title, author) =
        match tauri::async_runtime::spawn_blocking(move || state_arc.external_meta_inputs(id))
            .await
        {
            Ok(r) => r?,
            Err(e) => return Err(Error::Other(format!("meta inputs: {e}"))),
        };
    if lib_id.is_empty() {
        // No stable key → can't cache, so don't bother fetching.
        return Ok(BookExternalMeta {
            lib_id: String::new(),
            entries: Vec::new(),
            fetching: false,
        });
    }

    let state_arc = Arc::clone(state.inner());
    let cached_view = {
        let lib_id_cl = lib_id.clone();
        let state_for_view = Arc::clone(&state_arc);
        tauri::async_runtime::spawn_blocking(move || state_for_view.external_meta_view(&lib_id_cl, false))
            .await
            .map_err(|e| Error::Other(format!("meta view: {e}")))??
    };

    // Decide which sources need (re)fetching.
    let now = now_secs();
    let mut stale: Vec<&'static str> = Vec::new();
    for src in EXTERNAL_META_SOURCES {
        let cached = cached_view.entries.iter().find(|e| e.source == *src);
        match cached {
            Some(e) if now - e.fetched_at < EXTERNAL_META_TTL_SECS => {}
            _ => stale.push(src),
        }
    }
    if stale.is_empty() {
        return Ok(cached_view);
    }

    // Spawn one background worker per stale source. They each emit the same
    // aggregated event on completion so the UI rerenders progressively.
    for src in stale {
        let app = app.clone();
        let state_arc = Arc::clone(&state_arc);
        let title = title.clone();
        let author = author.clone();
        let lib_id_cl = lib_id.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let entry = match src {
                "google" => fetch_google_books(&title, author.as_deref()),
                "openlibrary" => fetch_openlibrary(&title, author.as_deref()),
                _ => return,
            };
            if let Err(e) = state_arc.put_external_meta(&lib_id_cl, &entry) {
                tracing::warn!("put_external_meta failed: {e}");
            }
            // Re-read full aggregated view so the event carries fresh data.
            let view = match state_arc.external_meta_view(&lib_id_cl, false) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!("external_meta_view failed: {e}");
                    return;
                }
            };
            let _ = app.emit("book-meta-updated", &view);
        });
    }

    Ok(BookExternalMeta {
        lib_id: cached_view.lib_id,
        entries: cached_view.entries,
        fetching: true,
    })
}

#[tauri::command]
pub fn list_lists(state: State<Arc<LibraryState>>) -> Result<Vec<UserList>> {
    state.list_lists()
}

#[tauri::command]
pub fn create_list(state: State<Arc<LibraryState>>, name: String) -> Result<UserList> {
    state.create_list(&name)
}

#[tauri::command]
pub fn rename_list(state: State<Arc<LibraryState>>, id: i64, name: String) -> Result<()> {
    state.rename_list(id, &name)
}

#[tauri::command]
pub fn delete_list(state: State<Arc<LibraryState>>, id: i64) -> Result<()> {
    state.delete_list(id)
}

#[tauri::command]
pub fn add_to_list(
    state: State<Arc<LibraryState>>,
    list_id: i64,
    kind: String,
    ref_key: String,
) -> Result<()> {
    state.add_to_list(list_id, &kind, &ref_key)
}

#[tauri::command]
pub fn remove_from_list(
    state: State<Arc<LibraryState>>,
    list_id: i64,
    kind: String,
    ref_key: String,
) -> Result<()> {
    state.remove_from_list(list_id, &kind, &ref_key)
}

#[tauri::command]
pub fn lists_containing(
    state: State<Arc<LibraryState>>,
    kind: String,
    ref_key: String,
) -> Result<Vec<i64>> {
    state.lists_containing(&kind, &ref_key)
}

#[tauri::command]
pub fn get_list_contents(state: State<Arc<LibraryState>>, id: i64) -> Result<ListContents> {
    state.list_contents(id)
}

/// Async wrapper that pushes the long-running import to a blocking worker
/// thread. Keeping the heavy work off the runtime is what lets the
/// `import-progress` events actually reach the webview while the parser is
/// still running.
#[tauri::command]
pub async fn import_inpx<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<LibraryState>>,
    path: String,
) -> Result<LibraryStats> {
    let p = PathBuf::from(path);
    let state = Arc::clone(state.inner());
    let join = tauri::async_runtime::spawn_blocking(move || state.import_inpx(&app, &p));
    join.await
        .map_err(|e| Error::Other(format!("import worker: {e}")))?
}

#[tauri::command]
pub async fn share_start<R: Runtime>(
    app: AppHandle<R>,
    library: State<'_, Arc<LibraryState>>,
    share: State<'_, Arc<ShareState>>,
    domain: Option<String>,
    pooling: Option<bool>,
) -> Result<ShareStatus> {
    let library = Arc::clone(library.inner());
    let share = Arc::clone(share.inner());
    let pooling = pooling.unwrap_or(false);
    let join = tauri::async_runtime::spawn_blocking(move || {
        share.start(&app, library, domain, pooling)
    });
    join.await
        .map_err(|e| Error::Other(format!("share worker: {e}")))?
}

#[tauri::command]
pub async fn share_stop<R: Runtime>(
    app: AppHandle<R>,
    share: State<'_, Arc<ShareState>>,
) -> Result<ShareStatus> {
    let share = Arc::clone(share.inner());
    let join = tauri::async_runtime::spawn_blocking(move || share.stop(&app));
    join.await
        .map_err(|e| Error::Other(format!("share stop worker: {e}")))?
}

#[tauri::command]
pub fn share_status(share: State<Arc<ShareState>>) -> ShareStatus {
    share.status()
}

#[tauri::command]
pub async fn share_kill_stray(share: State<'_, Arc<ShareState>>) -> Result<u32> {
    let running = share.status().running;
    let join = tauri::async_runtime::spawn_blocking(move || crate::share::kill_stray_ngrok(running));
    join.await
        .map_err(|e| Error::Other(format!("kill stray worker: {e}")))?
}

#[tauri::command]
pub async fn share_list_domains() -> Result<Vec<String>> {
    let join = tauri::async_runtime::spawn_blocking(crate::share::list_reserved_domains);
    join.await
        .map_err(|e| Error::Other(format!("list domains worker: {e}")))
}

#[tauri::command]
pub async fn export_books<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, Arc<LibraryState>>,
    book_ids: Vec<i64>,
    target_dir: String,
) -> Result<ExportSummary> {
    let target = PathBuf::from(target_dir);
    let state = Arc::clone(state.inner());
    let join =
        tauri::async_runtime::spawn_blocking(move || state.export_books(&app, &book_ids, &target));
    join.await
        .map_err(|e| Error::Other(format!("export worker: {e}")))?
}
