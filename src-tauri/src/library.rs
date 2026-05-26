use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::error::{Error, Result};
use crate::export::{copy_book_from_zip, target_path_for};
use crate::fb2::read_book_content;
use crate::index::LibraryIndex;
use crate::inpx::{compute_inp_byte_total, parse_inpx, read_metadata};
use crate::model::{
    ArchiveHit, AuthorHit, AuthorView, Book, BookContent, BookExternalMeta, BookFilters,
    BookListItem, ExportError, ExportProgress, ExportSummary, ExternalMetaEntry, GenreHit,
    ImportProgress, InpRecord, LanguageHit, LibraryStats, ListContents, ReaderBook,
    ReadingPosition, SearchResults, SeriesGroup, SeriesHit, UserList,
};
use crate::reader::read_reader_book;

const BATCH_SIZE: usize = 1000;
const ARCHIVE_SCAN_DEPTH: usize = 3;

pub struct LibraryState {
    db_path: PathBuf,
    index: Arc<Mutex<LibraryIndex>>,
    /// Cached directory that contains the companion `.zip` archives. Populated
    /// lazily the first time we resolve a book file — Flibusta-style flash
    /// drives often keep the books in a subdirectory rather than next to the
    /// INPX itself.
    books_dir: Mutex<Option<PathBuf>>,
}

impl LibraryState {
    pub fn open(db_path: PathBuf) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut index = LibraryIndex::open(&db_path)?;
        index.ensure_builtin_lists()?;
        let cached_dir = index
            .get_meta("books_dir")?
            .map(PathBuf::from)
            .filter(|p| p.is_dir());
        Ok(Self {
            db_path,
            index: Arc::new(Mutex::new(index)),
            books_dir: Mutex::new(cached_dir),
        })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, LibraryIndex>> {
        self.index.lock().map_err(|_| poison())
    }

    pub fn stats(&self) -> Result<LibraryStats> {
        self.lock()?.stats()
    }

    pub fn list_books(
        &self,
        query: Option<&str>,
        filters: &BookFilters,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<BookListItem>> {
        self.lock()?.list_books(query, filters, limit, offset)
    }

    pub fn get_book(&self, id: i64) -> Result<Option<Book>> {
        self.lock()?.get_book(id)
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        self.lock()?.get_meta(key)
    }

    pub fn list_languages(&self, filters: &BookFilters) -> Result<Vec<LanguageHit>> {
        let raw = self.lock()?.list_languages(filters)?;
        Ok(raw
            .into_iter()
            .map(|(code, count)| LanguageHit { code, count })
            .collect())
    }

    pub fn list_genres(&self, filters: &BookFilters) -> Result<Vec<GenreHit>> {
        self.lock()?.list_genres(filters)
    }

    pub fn list_archives(&self) -> Result<Vec<ArchiveHit>> {
        self.lock()?.list_archives()
    }

    pub fn search(
        &self,
        query: &str,
        include_authors: bool,
        include_series: bool,
        include_books: bool,
        filters: &BookFilters,
        limit_per_group: i64,
    ) -> Result<SearchResults> {
        let q = query.trim();
        let idx = self.lock()?;
        let authors = if include_authors && !q.is_empty() {
            idx.search_authors(q, filters, limit_per_group)?
        } else {
            vec![]
        };
        let series = if include_series && !q.is_empty() {
            idx.search_series(q, filters, limit_per_group)?
        } else {
            vec![]
        };
        let books = if include_books {
            idx.list_books(
                if q.is_empty() { None } else { Some(q) },
                filters,
                limit_per_group,
                0,
            )?
        } else {
            vec![]
        };
        Ok(SearchResults {
            authors,
            series,
            books,
        })
    }

    pub fn author_view(&self, author_id: i64, filters: &BookFilters) -> Result<AuthorView> {
        let idx = self.lock()?;
        let display = idx
            .author_display(author_id)?
            .ok_or_else(|| Error::NotFound(format!("автор {author_id}")))?;
        let books = idx.books_by_author(author_id, filters)?;
        drop(idx);
        Ok(AuthorView {
            id: author_id,
            display,
            groups: group_books_by_series(books),
        })
    }

    pub fn series_view(
        &self,
        name: &str,
        filters: &BookFilters,
    ) -> Result<Vec<BookListItem>> {
        self.lock()?.books_by_series(name, filters)
    }

    pub fn books_by_author(
        &self,
        author_id: i64,
        filters: &BookFilters,
    ) -> Result<Vec<BookListItem>> {
        self.lock()?.books_by_author(author_id, filters)
    }

    pub fn list_authors(&self, offset: i64, limit: i64) -> Result<Vec<AuthorHit>> {
        self.lock()?.list_all_authors(offset, limit)
    }

    pub fn count_authors(&self) -> Result<i64> {
        self.lock()?.count_authors()
    }

    pub fn list_series(&self, offset: i64, limit: i64) -> Result<Vec<SeriesHit>> {
        self.lock()?.list_all_series(offset, limit)
    }

    pub fn count_series(&self) -> Result<i64> {
        self.lock()?.count_series()
    }

    pub fn author_display(&self, author_id: i64) -> Result<Option<String>> {
        self.lock()?.author_display(author_id)
    }

    pub fn author_id_by_display(&self, display: &str) -> Result<Option<i64>> {
        self.lock()?.author_id_by_display(display)
    }

    pub fn author_first_letters(
        &self,
        filters: &BookFilters,
    ) -> Result<Vec<(String, i64)>> {
        self.lock()?.author_first_letters(filters)
    }

    pub fn series_first_letters(
        &self,
        filters: &BookFilters,
    ) -> Result<Vec<(String, i64)>> {
        self.lock()?.series_first_letters(filters)
    }

    pub fn series_by_prefix(
        &self,
        filters: &BookFilters,
        prefix: &str,
    ) -> Result<Vec<SeriesHit>> {
        self.lock()?.series_by_prefix(filters, prefix)
    }

    pub fn author_two_letter_prefixes(
        &self,
        filters: &BookFilters,
        letter: &str,
    ) -> Result<Vec<(String, i64)>> {
        self.lock()?.author_two_letter_prefixes(filters, letter)
    }

    pub fn authors_by_prefix(
        &self,
        filters: &BookFilters,
        prefix: &str,
    ) -> Result<Vec<AuthorHit>> {
        self.lock()?.authors_by_prefix(filters, prefix)
    }

    pub fn search_authors(
        &self,
        query: &str,
        filters: &BookFilters,
        limit: i64,
    ) -> Result<Vec<AuthorHit>> {
        self.lock()?.search_authors(query, filters, limit)
    }

    pub fn search_series(
        &self,
        query: &str,
        filters: &BookFilters,
        limit: i64,
    ) -> Result<Vec<SeriesHit>> {
        self.lock()?.search_series(query, filters, limit)
    }

    /// Read the raw book file bytes from its companion zip. Returns
    /// (bytes, suggested filename, content-type).
    pub fn read_book_bytes(&self, book_id: i64) -> Result<(Vec<u8>, String, String)> {
        let book = self
            .get_book(book_id)?
            .ok_or_else(|| Error::NotFound(format!("книга {book_id}")))?;
        let zip_path = self.resolve_archive(&book.archive)?;
        let zf = std::fs::File::open(&zip_path)?;
        let mut zip = zip::ZipArchive::new(zf)?;
        let ext = if book.ext.is_empty() {
            "fb2".to_string()
        } else {
            book.ext.clone()
        };
        let candidates = [
            format!("{}.{}", book.file, ext),
            format!("{}.{}", book.file, ext.to_ascii_lowercase()),
            format!("{}.{}", book.file, ext.to_ascii_uppercase()),
            format!("{}.fb2", book.file),
        ];
        let mut buf = Vec::new();
        let mut hit_name: Option<String> = None;
        for name in &candidates {
            if let Ok(mut entry) = zip.by_name(name) {
                use std::io::Read;
                entry.read_to_end(&mut buf)?;
                hit_name = Some(name.clone());
                break;
            }
        }
        if hit_name.is_none() {
            return Err(Error::NotFound(format!(
                "файл {}.{} не найден в {}",
                book.file,
                ext,
                zip_path.display()
            )));
        }
        let stem = if book.title.is_empty() {
            book.file.clone()
        } else {
            book.title.clone()
        };
        let suggested = format!("{}.{}", sanitize_basename(&stem), ext.to_ascii_lowercase());
        let content_type = match ext.to_ascii_lowercase().as_str() {
            "fb2" => "application/fb2+xml".to_string(),
            "epub" => "application/epub+zip".to_string(),
            "pdf" => "application/pdf".to_string(),
            "djvu" => "image/vnd.djvu".to_string(),
            "mobi" => "application/x-mobipocket-ebook".to_string(),
            "zip" => "application/zip".to_string(),
            _ => "application/octet-stream".to_string(),
        };
        Ok((buf, suggested, content_type))
    }

    /// Pull the cover image bytes (+ MIME) out of an FB2 book without
    /// loading the full body content. Returns `Ok(None)` if the book is not
    /// FB2 or doesn't have a `<coverpage>`. Fast enough to call per-entry
    /// when rendering an OPDS feed page.
    pub fn read_book_cover(&self, book_id: i64) -> Result<Option<(Vec<u8>, String)>> {
        let book = self
            .get_book(book_id)?
            .ok_or_else(|| Error::NotFound(format!("книга {book_id}")))?;
        if !book.ext.eq_ignore_ascii_case("fb2") {
            return Ok(None);
        }
        let (bytes, _name, _ctype) = self.read_book_bytes(book_id)?;
        crate::fb2::extract_cover(&bytes)
    }

    /// Read the book and, if it's FB2, transcode to EPUB. For non-FB2
    /// formats we just return the original bytes labelled as their native
    /// MIME — there's nothing to convert. Useful for OPDS clients (KyBook,
    /// etc.) that don't understand FB2.
    pub fn read_book_as_epub(&self, book_id: i64) -> Result<(Vec<u8>, String, String)> {
        let book = self
            .get_book(book_id)?
            .ok_or_else(|| Error::NotFound(format!("книга {book_id}")))?;
        let ext_lc = book.ext.to_ascii_lowercase();
        if ext_lc != "fb2" {
            // Not FB2 → return original. Caller decides what MIME to advertise.
            return self.read_book_bytes(book_id);
        }
        let (raw, _name, _ctype) = self.read_book_bytes(book_id)?;
        let identifier = if !book.lib_id.is_empty() {
            book.lib_id.clone()
        } else {
            format!("id-{}", book.id)
        };
        let conv = crate::fb2_epub::convert_fb2_to_epub(&raw, &identifier)?;
        let filename = format!("{}.epub", conv.filename_stem);
        Ok((conv.bytes, filename, "application/epub+zip".to_string()))
    }

    // ---- Lists -------------------------------------------------------

    pub fn list_lists(&self) -> Result<Vec<UserList>> {
        self.lock()?.list_lists()
    }

    pub fn create_list(&self, name: &str) -> Result<UserList> {
        self.lock()?.create_list(name)
    }

    pub fn rename_list(&self, id: i64, new_name: &str) -> Result<()> {
        self.lock()?.rename_list(id, new_name)
    }

    pub fn delete_list(&self, id: i64) -> Result<()> {
        self.lock()?.delete_list(id)
    }

    pub fn add_to_list(&self, list_id: i64, kind: &str, ref_key: &str) -> Result<()> {
        self.lock()?.add_to_list(list_id, kind, ref_key)
    }

    pub fn remove_from_list(&self, list_id: i64, kind: &str, ref_key: &str) -> Result<()> {
        self.lock()?.remove_from_list(list_id, kind, ref_key)
    }

    pub fn lists_containing(&self, kind: &str, ref_key: &str) -> Result<Vec<i64>> {
        self.lock()?.lists_containing(kind, ref_key)
    }

    pub fn list_contents(&self, list_id: i64) -> Result<ListContents> {
        self.lock()?.list_contents(list_id)
    }

    pub fn export_books<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        book_ids: &[i64],
        target_dir: &Path,
    ) -> Result<ExportSummary> {
        std::fs::create_dir_all(target_dir)?;

        let total = book_ids.len() as u64;
        let mut copied: u64 = 0;
        let mut skipped: u64 = 0;
        let mut errors: Vec<ExportError> = Vec::new();
        let mut last_emit = Instant::now();

        emit_export(app, "starting", 0, total, "");

        for (i, &id) in book_ids.iter().enumerate() {
            let book = match self.get_book(id)? {
                Some(b) => b,
                None => {
                    errors.push(ExportError {
                        book_id: id,
                        title: String::new(),
                        message: "книга не найдена в индексе".into(),
                    });
                    skipped += 1;
                    continue;
                }
            };
            let title = book.title.clone();
            let target = target_path_for(target_dir, &book);

            if last_emit.elapsed() > std::time::Duration::from_millis(80)
                || i as u64 == total - 1
            {
                emit_export(app, "copying", copied + skipped, total, &title);
                last_emit = Instant::now();
            }

            let zip_path = match self.resolve_archive(&book.archive) {
                Ok(p) => p,
                Err(e) => {
                    errors.push(ExportError {
                        book_id: id,
                        title,
                        message: e.to_string(),
                    });
                    skipped += 1;
                    continue;
                }
            };

            match copy_book_from_zip(&zip_path, &book.file, &book.ext, &target) {
                Ok(true) => copied += 1,
                Ok(false) => skipped += 1, // already there with matching size
                Err(e) => {
                    errors.push(ExportError {
                        book_id: id,
                        title,
                        message: e.to_string(),
                    });
                    skipped += 1;
                }
            }
        }

        emit_export(app, "done", total, total, "");

        Ok(ExportSummary {
            total,
            copied,
            skipped,
            target_dir: target_dir.to_string_lossy().to_string(),
            errors,
        })
    }

    pub fn book_content(&self, book_id: i64) -> Result<BookContent> {
        let (archive, file, ext) = {
            let idx = self.lock()?;
            let book = idx
                .get_book(book_id)?
                .ok_or_else(|| Error::NotFound(format!("книга {book_id}")))?;
            (book.archive, book.file, book.ext)
        };
        let zip_path = self.resolve_archive(&archive)?;
        read_book_content(&zip_path, &file, &ext)
    }

    pub fn reader_book(&self, book_id: i64) -> Result<ReaderBook> {
        let (archive, file, ext, lib_id) = {
            let idx = self.lock()?;
            let book = idx
                .get_book(book_id)?
                .ok_or_else(|| Error::NotFound(format!("книга {book_id}")))?;
            (book.archive, book.file, book.ext, book.lib_id)
        };
        let zip_path = self.resolve_archive(&archive)?;
        let mut rb = read_reader_book(&zip_path, &file, &ext)?;
        if !lib_id.is_empty() {
            rb.position = self.lock()?.get_reading_position(&lib_id)?;
        }
        Ok(rb)
    }

    pub fn save_reading_position(
        &self,
        lib_id: &str,
        chapter_id: &str,
        scroll: f64,
    ) -> Result<()> {
        self.lock()?
            .save_reading_position(lib_id, chapter_id, scroll)
    }

    pub fn reading_position(&self, lib_id: &str) -> Result<Option<ReadingPosition>> {
        self.lock()?.get_reading_position(lib_id)
    }

    // ---- External book metadata -----------------------------------------

    /// Resolve `(lib_id, title, primary_author)` for a given book id. Used by
    /// the external-meta lookups (Google Books / OpenLibrary).
    pub fn external_meta_inputs(
        &self,
        book_id: i64,
    ) -> Result<(String, String, Option<String>)> {
        let book = self
            .get_book(book_id)?
            .ok_or_else(|| Error::NotFound(format!("книга {book_id}")))?;
        let author = book.authors.first().map(|a| a.display());
        Ok((book.lib_id, book.title, author))
    }

    pub fn cached_external_meta(&self, lib_id: &str) -> Result<Vec<ExternalMetaEntry>> {
        self.lock()?.get_external_meta(lib_id)
    }

    pub fn put_external_meta(
        &self,
        lib_id: &str,
        entry: &ExternalMetaEntry,
    ) -> Result<()> {
        self.lock()?.put_external_meta(lib_id, entry)
    }

    pub fn external_meta_view(&self, lib_id: &str, fetching: bool) -> Result<BookExternalMeta> {
        Ok(BookExternalMeta {
            lib_id: lib_id.to_string(),
            entries: self.cached_external_meta(lib_id)?,
            fetching,
        })
    }

    pub fn resolve_archive(&self, archive: &str) -> Result<PathBuf> {
        // 1. cached books_dir
        {
            let g = self.books_dir.lock().unwrap();
            if let Some(d) = g.as_ref() {
                let p = d.join(archive);
                if p.exists() {
                    return Ok(p);
                }
            }
        }
        // 2. INPX directory
        let inpx_path = self
            .get_meta("inpx_path")?
            .ok_or_else(|| Error::Other("INPX не загружен".into()))?;
        let inpx_dir = PathBuf::from(&inpx_path)
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| Error::Other(format!("не понимаю путь INPX: {inpx_path}")))?;
        let direct = inpx_dir.join(archive);
        if direct.exists() {
            self.cache_books_dir(&inpx_dir)?;
            return Ok(direct);
        }
        // 3. recursive scan
        if let Some(found) = find_archive_recursive(&inpx_dir, archive, ARCHIVE_SCAN_DEPTH) {
            if let Some(parent) = found.parent() {
                self.cache_books_dir(parent)?;
            }
            return Ok(found);
        }
        Err(Error::NotFound(format!(
            "архив {archive} не найден ни в {}, ни в подпапках (глубина {ARCHIVE_SCAN_DEPTH})",
            inpx_dir.display()
        )))
    }

    fn cache_books_dir(&self, dir: &Path) -> Result<()> {
        {
            let mut g = self.books_dir.lock().unwrap();
            *g = Some(dir.to_path_buf());
        }
        let mut idx = self.lock()?;
        idx.set_meta("books_dir", &dir.to_string_lossy())?;
        Ok(())
    }

    pub fn import_inpx<R: Runtime>(&self, app: &AppHandle<R>, path: &Path) -> Result<LibraryStats> {
        let meta = read_metadata(path)?;
        {
            let mut g = self.lock()?;
            g.clear()?;
            g.set_meta("inpx_path", &path.to_string_lossy())?;
            g.set_meta("collection_name", &meta.collection_name)?;
            g.set_meta("collection_version", &meta.collection_version)?;
        }
        {
            let mut g = self.books_dir.lock().unwrap();
            *g = None;
        }

        let bytes_total = compute_inp_byte_total(path).unwrap_or(0);
        emit_progress(app, "reading", 0, bytes_total, 0);

        let batch: RefCell<Vec<InpRecord>> = RefCell::new(Vec::with_capacity(BATCH_SIZE));
        let records = Cell::new(0u64);
        let bytes_done = Cell::new(0u64);
        let last_emit = Cell::new(Instant::now());

        let maybe_emit = |force: bool| {
            if force || last_emit.get().elapsed() > std::time::Duration::from_millis(80) {
                emit_progress(app, "indexing", bytes_done.get(), bytes_total, records.get());
                last_emit.set(Instant::now());
            }
        };

        parse_inpx(
            path,
            |rec| {
                let mut b = batch.borrow_mut();
                b.push(rec);
                if b.len() >= BATCH_SIZE {
                    let mut g = self.index.lock().map_err(|_| poison())?;
                    g.import_batch(&b)?;
                    records.set(records.get() + b.len() as u64);
                    b.clear();
                    drop(g);
                    drop(b);
                    maybe_emit(false);
                }
                Ok(())
            },
            |inp_bytes| {
                bytes_done.set(bytes_done.get().saturating_add(inp_bytes));
                maybe_emit(false);
            },
        )?;

        let mut remainder = batch.borrow_mut();
        if !remainder.is_empty() {
            let mut g = self.lock()?;
            g.import_batch(&remainder)?;
            records.set(records.get() + remainder.len() as u64);
            remainder.clear();
        }
        drop(remainder);

        emit_progress(app, "done", bytes_total, bytes_total, records.get());
        self.stats()
    }
}

fn poison() -> Error {
    Error::Other("index lock poisoned".into())
}

fn sanitize_basename(s: &str) -> String {
    let bad: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];
    let cleaned: String = s
        .chars()
        .map(|c| if bad.contains(&c) || (c as u32) < 0x20 { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "book".to_string()
    } else if trimmed.len() > 180 {
        let mut cut = 180;
        while !trimmed.is_char_boundary(cut) {
            cut -= 1;
        }
        trimmed[..cut].trim_end().to_string()
    } else {
        trimmed
    }
}

fn find_archive_recursive(start: &Path, archive: &str, depth: usize) -> Option<PathBuf> {
    let direct = start.join(archive);
    if direct.exists() {
        return Some(direct);
    }
    if depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(start).ok()?;
    for e in entries.flatten() {
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if e.file_type().ok().map(|t| t.is_dir()).unwrap_or(false) {
            if let Some(p) = find_archive_recursive(&e.path(), archive, depth - 1) {
                return Some(p);
            }
        }
    }
    None
}

fn group_books_by_series(books: Vec<BookListItem>) -> Vec<SeriesGroup> {
    let mut groups: Vec<SeriesGroup> = Vec::new();
    let mut current_name: Option<Option<String>> = None;
    for b in books {
        let name = b
            .series
            .as_ref()
            .filter(|s| !s.is_empty())
            .cloned();
        match &current_name {
            Some(cur) if cur == &name => {
                groups.last_mut().unwrap().books.push(b);
            }
            _ => {
                current_name = Some(name.clone());
                groups.push(SeriesGroup {
                    name,
                    books: vec![b],
                });
            }
        }
    }
    groups
}

fn emit_progress<R: Runtime>(
    app: &AppHandle<R>,
    stage: &str,
    bytes_done: u64,
    bytes_total: u64,
    records: u64,
) {
    let p = ImportProgress {
        stage: stage.to_string(),
        bytes_done,
        bytes_total,
        records,
    };
    let _ = app.emit("import-progress", p);
}

fn emit_export<R: Runtime>(
    app: &AppHandle<R>,
    stage: &str,
    done: u64,
    total: u64,
    current: &str,
) {
    let p = ExportProgress {
        stage: stage.to_string(),
        done,
        total,
        current: current.to_string(),
    };
    let _ = app.emit("export-progress", p);
}

pub fn default_db_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| Error::Other(format!("app_data_dir: {e}")))?;
    Ok(dir.join("library.db"))
}
