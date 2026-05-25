use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct AuthorName {
    pub last: String,
    pub first: String,
    pub middle: String,
}

impl AuthorName {
    pub fn display(&self) -> String {
        let mut s = self.last.clone();
        if !self.first.is_empty() {
            if !s.is_empty() {
                s.push(' ');
            }
            s.push_str(&self.first);
        }
        if !self.middle.is_empty() {
            if !s.is_empty() {
                s.push(' ');
            }
            s.push_str(&self.middle);
        }
        s
    }

    pub fn is_empty(&self) -> bool {
        self.last.is_empty() && self.first.is_empty() && self.middle.is_empty()
    }
}

/// A single record from an .inp file inside an INPX archive.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InpRecord {
    pub authors: Vec<AuthorName>,
    pub genres: Vec<String>,
    pub title: String,
    pub series: String,
    pub ser_no: Option<u32>,
    pub file: String,
    pub size: u64,
    pub lib_id: String,
    pub deleted: bool,
    pub ext: String,
    pub date: String,
    pub lang: String,
    pub librate: Option<u32>,
    pub keywords: String,
    /// Companion archive (`.zip`) holding the actual book file.
    pub archive: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Book {
    pub id: i64,
    pub title: String,
    pub authors: Vec<AuthorName>,
    pub genres: Vec<String>,
    pub series: Option<String>,
    pub ser_no: Option<u32>,
    pub size: u64,
    pub lang: String,
    pub librate: Option<u32>,
    pub date: String,
    pub ext: String,
    pub file: String,
    pub archive: String,
    pub lib_id: String,
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookListItem {
    pub id: i64,
    pub lib_id: String,
    pub title: String,
    pub authors: String,
    pub series: Option<String>,
    pub ser_no: Option<u32>,
    pub lang: String,
    pub size: u64,
    pub ext: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImportProgress {
    pub stage: String,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub records: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStats {
    pub books: u64,
    pub authors: u64,
    pub series: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorHit {
    pub id: i64,
    pub display: String,
    pub book_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesHit {
    pub name: String,
    pub book_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResults {
    pub authors: Vec<AuthorHit>,
    pub series: Vec<SeriesHit>,
    pub books: Vec<BookListItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SeriesGroup {
    pub name: Option<String>,
    pub books: Vec<BookListItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthorView {
    pub id: i64,
    pub display: String,
    pub groups: Vec<SeriesGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BookContent {
    pub description: String,
    pub cover_data_url: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageHit {
    pub code: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenreHit {
    pub code: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveHit {
    pub name: String,
    pub count: u32,
}

/// Shared filter payload accepted by `list_books`, `search`, and the alphabet
/// index queries. Every field is optional — an empty `BookFilters` matches
/// the whole catalog. `genre` is the genre `code` (e.g. `"sf_history"`),
/// `archive` is the companion `.zip` name (e.g. `"fb.lib.001-100.zip"`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookFilters {
    pub lang: Option<String>,
    pub genre: Option<String>,
    pub archive: Option<String>,
    pub author_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserList {
    pub id: i64,
    pub name: String,
    pub builtin: bool,
    pub item_count: u32,
}

/// A resolved list view: items split by kind, joined against current catalog
/// data so we can show book titles, author names with book counts, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListContents {
    pub list: UserList,
    pub books: Vec<BookListItem>,
    pub authors: Vec<AuthorHit>,
    pub series: Vec<SeriesHit>,
    /// Items that no longer resolve in the current catalog (book lib_id or
    /// author display not found). Surfaced so the user can clean them up.
    pub orphans: Vec<OrphanItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphanItem {
    pub kind: String,
    pub ref_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderBook {
    pub title: String,
    pub authors: Vec<String>,
    pub lang: String,
    pub cover_data_url: Option<String>,
    pub chapters: Vec<ReaderChapter>,
    pub toc: Vec<TocEntry>,
    /// "fb2" or "epub". Format-specific styling on the frontend.
    pub format: String,
    /// Last persisted reading position for this book, if any.
    pub position: Option<ReadingPosition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderChapter {
    /// Stable id (e.g. "ch0"); referenced by TOC entries and the position
    /// store so frontend can deep-link / restore scroll.
    pub id: String,
    pub title: Option<String>,
    pub html: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TocEntry {
    pub title: String,
    pub chapter_id: String,
    pub anchor: Option<String>,
    pub children: Vec<TocEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingPosition {
    pub lib_id: String,
    pub chapter_id: String,
    /// 0.0..=1.0 fraction of how far into the chapter the user scrolled.
    pub scroll: f64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExportProgress {
    pub stage: String,
    pub done: u64,
    pub total: u64,
    pub current: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExportError {
    pub book_id: i64,
    pub title: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExportSummary {
    pub total: u64,
    pub copied: u64,
    pub skipped: u64,
    pub target_dir: String,
    pub errors: Vec<ExportError>,
}

/// A single external metadata snapshot from one provider (Google Books,
/// OpenLibrary, ...). Stored per-source in `book_external_meta` so the UI can
/// merge or show provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalMetaEntry {
    pub source: String,
    pub status: String,
    pub description: Option<String>,
    pub rating: Option<f64>,
    pub rating_count: Option<i64>,
    pub url: Option<String>,
    pub fetched_at: i64,
}

/// Aggregate of all external sources for one book, ready for the BookDetail
/// pane. Ratings are kept per-source so the UI can show "Google: 4.3 · OL: 4.0".
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BookExternalMeta {
    pub lib_id: String,
    pub entries: Vec<ExternalMetaEntry>,
    /// True when at least one source is being refreshed in the background.
    pub fetching: bool,
}
