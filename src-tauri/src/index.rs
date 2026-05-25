use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Row};

use crate::error::{Error, Result};
use crate::model::{
    ArchiveHit, AuthorHit, AuthorName, Book, BookFilters, BookListItem, GenreHit, InpRecord,
    LibraryStats, ListContents, OrphanItem, SeriesHit, UserList,
};

const SCHEMA: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS meta (
  key   TEXT PRIMARY KEY,
  value TEXT
);

CREATE TABLE IF NOT EXISTS books (
  id        INTEGER PRIMARY KEY,
  title     TEXT NOT NULL,
  series    TEXT,
  ser_no    INTEGER,
  file      TEXT NOT NULL,
  archive   TEXT NOT NULL,
  size      INTEGER NOT NULL DEFAULT 0,
  lib_id    TEXT,
  deleted   INTEGER NOT NULL DEFAULT 0,
  ext       TEXT NOT NULL DEFAULT 'fb2',
  date      TEXT,
  lang      TEXT,
  librate   INTEGER,
  keywords  TEXT
);
CREATE INDEX IF NOT EXISTS idx_books_series  ON books(series) WHERE series IS NOT NULL AND series <> '';
CREATE INDEX IF NOT EXISTS idx_books_lang    ON books(lang)   WHERE lang IS NOT NULL AND lang <> '';
CREATE INDEX IF NOT EXISTS idx_books_archive ON books(archive);

CREATE TABLE IF NOT EXISTS authors (
  id      INTEGER PRIMARY KEY,
  last    TEXT NOT NULL DEFAULT '',
  first   TEXT NOT NULL DEFAULT '',
  middle  TEXT NOT NULL DEFAULT '',
  display TEXT NOT NULL,
  UNIQUE(last, first, middle)
);
CREATE INDEX IF NOT EXISTS idx_authors_display ON authors(display);

CREATE TABLE IF NOT EXISTS book_authors (
  book_id   INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  author_id INTEGER NOT NULL REFERENCES authors(id),
  PRIMARY KEY (book_id, author_id)
);
CREATE INDEX IF NOT EXISTS idx_book_authors_author ON book_authors(author_id);

CREATE TABLE IF NOT EXISTS genres (
  id   INTEGER PRIMARY KEY,
  code TEXT UNIQUE NOT NULL
);

CREATE TABLE IF NOT EXISTS book_genres (
  book_id  INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  genre_id INTEGER NOT NULL REFERENCES genres(id),
  PRIMARY KEY (book_id, genre_id)
);
CREATE INDEX IF NOT EXISTS idx_book_genres_genre ON book_genres(genre_id);

CREATE VIRTUAL TABLE IF NOT EXISTS books_fts USING fts5(
  title,
  authors,
  series,
  tokenize = 'unicode61 remove_diacritics 2'
);

CREATE VIRTUAL TABLE IF NOT EXISTS authors_fts USING fts5(
  display,
  tokenize = 'unicode61 remove_diacritics 2'
);

CREATE TABLE IF NOT EXISTS lists (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL COLLATE NOCASE,
  builtin    INTEGER NOT NULL DEFAULT 0,
  position   INTEGER NOT NULL DEFAULT 0,
  created_at INTEGER NOT NULL,
  UNIQUE(name)
);

CREATE TABLE IF NOT EXISTS list_items (
  id         INTEGER PRIMARY KEY,
  list_id    INTEGER NOT NULL REFERENCES lists(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL,
  ref_key    TEXT NOT NULL,
  added_at   INTEGER NOT NULL,
  UNIQUE(list_id, kind, ref_key)
);
CREATE INDEX IF NOT EXISTS idx_list_items_list ON list_items(list_id);

CREATE TABLE IF NOT EXISTS reading_progress (
  lib_id     TEXT PRIMARY KEY,
  chapter_id TEXT NOT NULL,
  scroll     REAL NOT NULL DEFAULT 0,
  updated_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS book_external_meta (
  lib_id       TEXT NOT NULL,
  source       TEXT NOT NULL,
  status       TEXT NOT NULL,
  description  TEXT,
  rating       REAL,
  rating_count INTEGER,
  url          TEXT,
  fetched_at   INTEGER NOT NULL,
  PRIMARY KEY (lib_id, source)
);
CREATE INDEX IF NOT EXISTS idx_book_external_meta_lib ON book_external_meta(lib_id);
"#;

pub struct LibraryIndex {
    conn: Connection,
}

impl LibraryIndex {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path.as_ref())?;
        conn.execute_batch(SCHEMA)?;
        register_text_udfs(&conn)?;
        Ok(Self { conn })
    }

    /// Wipe the catalog ahead of a fresh import — but keep user lists alive so
    /// favourites and to-read persist across re-indexing the INPX.
    pub fn clear(&mut self) -> Result<()> {
        self.conn.execute_batch(
            "DELETE FROM book_authors; DELETE FROM book_genres;
             DELETE FROM books_fts; DELETE FROM authors_fts;
             DELETE FROM books; DELETE FROM authors; DELETE FROM genres; DELETE FROM meta;",
        )?;
        Ok(())
    }

    pub fn set_meta(&mut self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO meta(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM meta WHERE key = ?1", params![key], |r| {
                r.get::<_, String>(0)
            })
            .optional()?)
    }

    pub fn stats(&self) -> Result<LibraryStats> {
        let books: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM books", [], |r| r.get(0))?;
        let authors: u64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM authors", [], |r| r.get(0))?;
        let series: u64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT series) FROM books WHERE series IS NOT NULL AND series <> ''",
            [],
            |r| r.get(0),
        )?;
        Ok(LibraryStats {
            books,
            authors,
            series,
        })
    }

    /// Bulk-insert a batch of records inside a single transaction.
    pub fn import_batch(&mut self, batch: &[InpRecord]) -> Result<()> {
        let tx = self.conn.transaction()?;
        {
            let mut insert_book = tx.prepare_cached(
                "INSERT INTO books(title, series, ser_no, file, archive, size, lib_id, deleted, ext, date, lang, librate, keywords)
                 VALUES(?1, NULLIF(?2, ''), ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            )?;
            let mut find_author = tx.prepare_cached(
                "SELECT id FROM authors WHERE last = ?1 AND first = ?2 AND middle = ?3",
            )?;
            let mut insert_author = tx.prepare_cached(
                "INSERT INTO authors(last, first, middle, display) VALUES(?1, ?2, ?3, ?4)",
            )?;
            let mut link_author = tx.prepare_cached(
                "INSERT OR IGNORE INTO book_authors(book_id, author_id) VALUES(?1, ?2)",
            )?;
            let mut find_genre = tx.prepare_cached("SELECT id FROM genres WHERE code = ?1")?;
            let mut insert_genre = tx.prepare_cached("INSERT INTO genres(code) VALUES(?1)")?;
            let mut link_genre =
                tx.prepare_cached("INSERT OR IGNORE INTO book_genres(book_id, genre_id) VALUES(?1, ?2)")?;
            let mut insert_fts = tx.prepare_cached(
                "INSERT INTO books_fts(rowid, title, authors, series) VALUES(?1, ?2, ?3, ?4)",
            )?;
            let mut insert_author_fts = tx.prepare_cached(
                "INSERT INTO authors_fts(rowid, display) VALUES(?1, ?2)",
            )?;

            for rec in batch {
                insert_book.execute(params![
                    rec.title,
                    rec.series,
                    rec.ser_no,
                    rec.file,
                    rec.archive,
                    rec.size as i64,
                    rec.lib_id,
                    rec.deleted as i64,
                    rec.ext,
                    rec.date,
                    rec.lang,
                    rec.librate,
                    rec.keywords,
                ])?;
                let book_id = tx.last_insert_rowid();

                let mut author_display_parts: Vec<String> = Vec::with_capacity(rec.authors.len());
                for a in &rec.authors {
                    let display = a.display();
                    author_display_parts.push(display.clone());
                    let aid: Option<i64> = find_author
                        .query_row(params![a.last, a.first, a.middle], |r| r.get::<_, i64>(0))
                        .optional()?;
                    let aid = match aid {
                        Some(id) => id,
                        None => {
                            insert_author
                                .execute(params![a.last, a.first, a.middle, display])?;
                            let new_id = tx.last_insert_rowid();
                            insert_author_fts.execute(params![new_id, display])?;
                            new_id
                        }
                    };
                    link_author.execute(params![book_id, aid])?;
                }

                for g in &rec.genres {
                    let gid: Option<i64> = find_genre
                        .query_row(params![g], |r| r.get::<_, i64>(0))
                        .optional()?;
                    let gid = match gid {
                        Some(id) => id,
                        None => {
                            insert_genre.execute(params![g])?;
                            tx.last_insert_rowid()
                        }
                    };
                    link_genre.execute(params![book_id, gid])?;
                }

                let authors_text = author_display_parts.join(", ");
                insert_fts.execute(params![book_id, rec.title, authors_text, rec.series])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_books(
        &self,
        query: Option<&str>,
        filters: &BookFilters,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<BookListItem>> {
        let limit = limit.clamp(1, 1000);
        let offset = offset.max(0);
        let (filter_clause, filter_vals) = filter_sql(filters);

        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        let sql = if let Some(q) = query.map(str::trim).filter(|s| !s.is_empty()) {
            params.push(Box::new(build_fts_query(q)));
            for v in filter_vals {
                params.push(v);
            }
            params.push(Box::new(limit));
            params.push(Box::new(offset));
            format!(
                "SELECT {COLS}
                 FROM books_fts f
                 JOIN books b ON b.id = f.rowid
                 WHERE books_fts MATCH ? AND b.deleted = 0{filter_clause}
                 ORDER BY rank
                 LIMIT ? OFFSET ?"
            )
        } else {
            for v in filter_vals {
                params.push(v);
            }
            params.push(Box::new(limit));
            params.push(Box::new(offset));
            format!(
                "SELECT {COLS}
                 FROM books b
                 WHERE b.deleted = 0{filter_clause}
                 ORDER BY b.id
                 LIMIT ? OFFSET ?"
            )
        };

        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            map_book_row,
        )?;
        let mut items = Vec::new();
        for r in rows {
            items.push(r?);
        }
        Ok(items)
    }

    pub fn get_book(&self, id: i64) -> Result<Option<Book>> {
        let book = self
            .conn
            .query_row(
                "SELECT id, title, series, ser_no, file, archive, size, lib_id, deleted, ext, date, lang, librate
                 FROM books WHERE id = ?1",
                params![id],
                |row| {
                    Ok(Book {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        authors: vec![],
                        genres: vec![],
                        series: row.get::<_, Option<String>>(2)?,
                        ser_no: row.get::<_, Option<u32>>(3)?,
                        file: row.get(4)?,
                        archive: row.get(5)?,
                        size: row.get::<_, i64>(6)? as u64,
                        lib_id: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
                        deleted: row.get::<_, i64>(8)? != 0,
                        ext: row.get(9)?,
                        date: row.get::<_, Option<String>>(10)?.unwrap_or_default(),
                        lang: row.get::<_, Option<String>>(11)?.unwrap_or_default(),
                        librate: row.get::<_, Option<u32>>(12)?,
                    })
                },
            )
            .optional()?;

        let Some(mut book) = book else {
            return Ok(None);
        };

        let mut stmt = self.conn.prepare(
            "SELECT a.last, a.first, a.middle FROM authors a
             JOIN book_authors ba ON ba.author_id = a.id
             WHERE ba.book_id = ?1
             ORDER BY a.display",
        )?;
        let rows = stmt.query_map(params![id], |row| {
            Ok(AuthorName {
                last: row.get(0)?,
                first: row.get(1)?,
                middle: row.get(2)?,
            })
        })?;
        for r in rows {
            book.authors.push(r?);
        }

        let mut stmt = self
            .conn
            .prepare("SELECT g.code FROM genres g JOIN book_genres bg ON bg.genre_id = g.id WHERE bg.book_id = ?1")?;
        let rows = stmt.query_map(params![id], |row| row.get::<_, String>(0))?;
        for r in rows {
            book.genres.push(r?);
        }

        Ok(Some(book))
    }

    pub fn search_authors(
        &self,
        query: &str,
        filters: &BookFilters,
        limit: i64,
    ) -> Result<Vec<AuthorHit>> {
        let limit = limit.clamp(1, 200);
        let fts_query = build_fts_query(query);
        if fts_query.is_empty() {
            return Ok(vec![]);
        }
        let (filter_clause, filter_vals) = filter_sql(filters);
        // Match each candidate author against the FTS hit, then count their
        // books *after* applying the current filter set so authors with no
        // matching books drop out via the WHERE book_count > 0 check below.
        let sql = format!(
            "SELECT a.id, a.display,
                    (SELECT COUNT(*) FROM book_authors ba
                       JOIN books b ON b.id = ba.book_id
                       WHERE ba.author_id = a.id AND b.deleted = 0{filter_clause}) AS book_count
             FROM authors_fts f
             JOIN authors a ON a.id = f.rowid
             WHERE authors_fts MATCH ?
             ORDER BY rank
             LIMIT ?"
        );
        // SQLite numbers `?` placeholders by textual order: the filter
        // bindings inside the book_count subquery come first, then MATCH,
        // then LIMIT. Pushing fts_query before filter_vals would silently
        // swap them and search would find nothing whenever a filter is set.
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        for v in filter_vals {
            params.push(v);
        }
        params.push(Box::new(fts_query));
        params.push(Box::new(limit));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| {
                Ok(AuthorHit {
                    id: row.get(0)?,
                    display: row.get(1)?,
                    book_count: row.get::<_, i64>(2)? as u32,
                })
            },
        )?;
        let any_filter = filters.lang.is_some()
            || filters.genre.is_some()
            || filters.archive.is_some()
            || filters.author_id.is_some();
        let mut out = Vec::new();
        for r in rows {
            let hit = r?;
            if any_filter && hit.book_count == 0 {
                continue;
            }
            out.push(hit);
        }
        Ok(out)
    }

    pub fn search_series(
        &self,
        query: &str,
        filters: &BookFilters,
        limit: i64,
    ) -> Result<Vec<SeriesHit>> {
        let limit = limit.clamp(1, 200);
        let pattern = format!("%{}%", query.trim().replace('%', "\\%").replace('_', "\\_"));
        if pattern == "%%" {
            return Ok(vec![]);
        }
        let (filter_clause, filter_vals) = filter_sql(filters);
        let sql = format!(
            "SELECT b.series, COUNT(*) AS c
             FROM books b
             WHERE b.series IS NOT NULL AND b.series <> '' AND b.deleted = 0
               AND b.series LIKE ? ESCAPE '\\'{filter_clause}
             GROUP BY b.series
             ORDER BY c DESC, b.series
             LIMIT ?"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        params.push(Box::new(pattern));
        for v in filter_vals {
            params.push(v);
        }
        params.push(Box::new(limit));
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| {
                Ok(SeriesHit {
                    name: row.get(0)?,
                    book_count: row.get::<_, i64>(1)? as u32,
                })
            },
        )?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn books_by_author(
        &self,
        author_id: i64,
        filters: &BookFilters,
    ) -> Result<Vec<BookListItem>> {
        // Apply filters to the author's books — but explicitly *not* the
        // author_id filter (we're already scoped to this author).
        let mut f = filters.clone();
        f.author_id = None;
        let (filter_clause, filter_vals) = filter_sql(&f);
        let sql = format!(
            "SELECT {COLS}
             FROM book_authors ba
             JOIN books b ON b.id = ba.book_id
             WHERE ba.author_id = ? AND b.deleted = 0{filter_clause}
             ORDER BY
               CASE WHEN b.series IS NULL OR b.series = '' THEN 1 ELSE 0 END,
               b.series COLLATE NOCASE,
               COALESCE(b.ser_no, 999999),
               b.title COLLATE NOCASE"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        params.push(Box::new(author_id));
        for v in filter_vals {
            params.push(v);
        }
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            map_book_row,
        )?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn books_by_series(
        &self,
        series: &str,
        filters: &BookFilters,
    ) -> Result<Vec<BookListItem>> {
        let (filter_clause, filter_vals) = filter_sql(filters);
        let sql = format!(
            "SELECT {COLS}
             FROM books b
             WHERE b.series = ? AND b.deleted = 0{filter_clause}
             ORDER BY COALESCE(b.ser_no, 999999), b.title COLLATE NOCASE"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        params.push(Box::new(series.to_string()));
        for v in filter_vals {
            params.push(v);
        }
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            map_book_row,
        )?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Languages with their book count under the active filter set.
    /// `filters.lang` is intentionally dropped — we want to see *all*
    /// languages even when a language filter is already set.
    pub fn list_languages(&self, filters: &BookFilters) -> Result<Vec<(String, u32)>> {
        let mut f = filters.clone();
        f.lang = None;
        let (filter_clause, filter_vals) = filter_sql(&f);
        let sql = format!(
            "SELECT b.lang, COUNT(*) AS c
             FROM books b
             WHERE b.lang IS NOT NULL AND b.lang <> '' AND b.deleted = 0{filter_clause}
             GROUP BY b.lang
             ORDER BY c DESC, b.lang"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(filter_vals.iter().map(|p| p.as_ref())),
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as u32)),
        )?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Genre codes with the count of non-deleted books they cover under the
    /// active filter set. `filters.genre` is dropped so the catalog stays
    /// browseable while a genre filter is already engaged.
    pub fn list_genres(&self, filters: &BookFilters) -> Result<Vec<GenreHit>> {
        let mut f = filters.clone();
        f.genre = None;
        let (filter_clause, filter_vals) = filter_sql(&f);
        let sql = format!(
            "SELECT g.code, COUNT(*) AS c
             FROM genres g
             JOIN book_genres bg ON bg.genre_id = g.id
             JOIN books b ON b.id = bg.book_id
             WHERE b.deleted = 0{filter_clause}
             GROUP BY g.code
             ORDER BY c DESC, g.code"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(filter_vals.iter().map(|p| p.as_ref())),
            |row| {
                Ok(GenreHit {
                    code: row.get::<_, String>(0)?,
                    count: row.get::<_, i64>(1)? as u32,
                })
            },
        )?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Distinct archive `.zip` names present in the catalog. These map to the
    /// physical folders/packs on disk; useful as a coarse "collection" filter.
    pub fn list_archives(&self) -> Result<Vec<ArchiveHit>> {
        let sql = "
            SELECT archive, COUNT(*) AS c FROM books
            WHERE archive IS NOT NULL AND archive <> '' AND deleted = 0
            GROUP BY archive
            ORDER BY archive";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(ArchiveHit {
                name: row.get::<_, String>(0)?,
                count: row.get::<_, i64>(1)? as u32,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn author_display(&self, author_id: i64) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row(
                "SELECT display FROM authors WHERE id = ?1",
                params![author_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?)
    }

    pub fn list_all_authors(&self, offset: i64, limit: i64) -> Result<Vec<AuthorHit>> {
        let limit = limit.clamp(1, 1000);
        let offset = offset.max(0);
        let sql = "
            SELECT a.id, a.display,
                   (SELECT COUNT(*) FROM book_authors ba WHERE ba.author_id = a.id) AS cnt
            FROM authors a
            ORDER BY a.display COLLATE NOCASE
            LIMIT ?1 OFFSET ?2";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![limit, offset], |row| {
            Ok(AuthorHit {
                id: row.get(0)?,
                display: row.get(1)?,
                book_count: row.get::<_, i64>(2)? as u32,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn count_authors(&self) -> Result<i64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM authors", [], |r| r.get::<_, i64>(0))?)
    }

    pub fn list_all_series(&self, offset: i64, limit: i64) -> Result<Vec<SeriesHit>> {
        let limit = limit.clamp(1, 1000);
        let offset = offset.max(0);
        let sql = "
            SELECT series, COUNT(*) AS c
            FROM books
            WHERE series IS NOT NULL AND series <> '' AND deleted = 0
            GROUP BY series
            ORDER BY series COLLATE NOCASE
            LIMIT ?1 OFFSET ?2";
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt.query_map(params![limit, offset], |row| {
            Ok(SeriesHit {
                name: row.get(0)?,
                book_count: row.get::<_, i64>(1)? as u32,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn count_series(&self) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COUNT(DISTINCT series) FROM books WHERE series IS NOT NULL AND series <> ''",
            [],
            |r| r.get::<_, i64>(0),
        )?)
    }

    /// Distinct first characters of `series` names, with the count of
    /// distinct series for each letter under the current filter set.
    pub fn series_first_letters(&self, filters: &BookFilters) -> Result<Vec<(String, i64)>> {
        let (filter_clause, filter_vals) = filter_sql(filters);
        let sql = format!(
            "SELECT first_letter(b.series) AS lt, COUNT(DISTINCT b.series) AS c
             FROM books b
             WHERE b.deleted = 0 AND b.series IS NOT NULL AND b.series <> ''
               AND first_letter(b.series) IS NOT NULL{filter_clause}
             GROUP BY lt
             ORDER BY lt"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(filter_vals.iter().map(|p| p.as_ref())),
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// All series whose normalized prefix matches `prefix` (case-insensitive,
    /// leading non-letters skipped). `prefix` should already be normalized
    /// — pass exactly what `first_letter` / `first_n_alpha` returned.
    pub fn series_by_prefix(
        &self,
        filters: &BookFilters,
        prefix: &str,
    ) -> Result<Vec<SeriesHit>> {
        let prefix_len = prefix.chars().count() as i64;
        let (filter_clause, filter_vals) = filter_sql(filters);
        let sql = format!(
            "SELECT b.series, COUNT(*) AS c
             FROM books b
             WHERE b.deleted = 0 AND b.series IS NOT NULL AND b.series <> ''
               AND first_n_alpha(b.series, ?) = ?{filter_clause}
             GROUP BY b.series
             ORDER BY b.series COLLATE NOCASE"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        params.push(Box::new(prefix_len));
        params.push(Box::new(prefix.to_string()));
        for v in filter_vals {
            params.push(v);
        }
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| {
                Ok(SeriesHit {
                    name: row.get::<_, String>(0)?,
                    book_count: row.get::<_, i64>(1)? as u32,
                })
            },
        )?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Distinct first characters of author `display` names, restricted to
    /// authors with at least one matching book under the current filter set.
    /// Returns `(letter, author_count)` sorted alphabetically.
    pub fn author_first_letters(&self, filters: &BookFilters) -> Result<Vec<(String, i64)>> {
        let (filter_clause, filter_vals) = filter_sql(filters);
        let sql = format!(
            "SELECT first_letter(a.display) AS lt, COUNT(DISTINCT a.id) AS c
             FROM authors a
             JOIN book_authors ba ON ba.author_id = a.id
             JOIN books b ON b.id = ba.book_id
             WHERE b.deleted = 0 AND a.display <> ''
               AND first_letter(a.display) IS NOT NULL{filter_clause}
             GROUP BY lt
             ORDER BY lt"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(filter_vals.iter().map(|p| p.as_ref())),
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn author_two_letter_prefixes(
        &self,
        filters: &BookFilters,
        letter: &str,
    ) -> Result<Vec<(String, i64)>> {
        let (filter_clause, filter_vals) = filter_sql(filters);
        let sql = format!(
            "SELECT first_n_alpha(a.display, 2) AS pfx, COUNT(DISTINCT a.id) AS c
             FROM authors a
             JOIN book_authors ba ON ba.author_id = a.id
             JOIN books b ON b.id = ba.book_id
             WHERE b.deleted = 0 AND a.display <> ''
               AND first_letter(a.display) = ?
               AND first_n_alpha(a.display, 2) IS NOT NULL{filter_clause}
             GROUP BY pfx
             ORDER BY pfx"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        params.push(Box::new(letter.to_string()));
        for v in filter_vals {
            params.push(v);
        }
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
        )?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn authors_by_prefix(
        &self,
        filters: &BookFilters,
        prefix: &str,
    ) -> Result<Vec<AuthorHit>> {
        let prefix_len = prefix.chars().count() as i64;
        let (filter_clause, filter_vals_a) = filter_sql(filters);
        let (_, filter_vals_b) = filter_sql(filters);
        // `b` is aliased so the filter EXISTS/JOIN both reference the same row.
        // Match the normalized prefix (uppercase first + lowercase rest, no
        // leading punctuation) so "А-я" and "а " bucket alongside "Аа".
        let sql = format!(
            "SELECT a.id, a.display,
                    (SELECT COUNT(*) FROM book_authors ba2
                       JOIN books b ON b.id = ba2.book_id
                       WHERE ba2.author_id = a.id AND b.deleted = 0{filter_clause}) AS cnt
             FROM authors a
             WHERE first_n_alpha(a.display, ?) = ?
               AND EXISTS (
                 SELECT 1 FROM book_authors ba
                   JOIN books b ON b.id = ba.book_id
                   WHERE ba.author_id = a.id AND b.deleted = 0{filter_clause}
               )
             ORDER BY a.display COLLATE NOCASE"
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        // Outer book_count subquery binds first.
        for v in filter_vals_a {
            params.push(v);
        }
        params.push(Box::new(prefix_len));
        params.push(Box::new(prefix.to_string()));
        // Inner EXISTS binds an independent copy of the same values.
        for v in filter_vals_b {
            params.push(v);
        }
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(
            rusqlite::params_from_iter(params.iter().map(|p| p.as_ref())),
            |row| {
                Ok(AuthorHit {
                    id: row.get(0)?,
                    display: row.get(1)?,
                    book_count: row.get::<_, i64>(2)? as u32,
                })
            },
        )?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn author_id_by_display(&self, display: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM authors WHERE display = ?1",
                params![display],
                |r| r.get::<_, i64>(0),
            )
            .optional()?)
    }

    // ----- User lists ---------------------------------------------------

    pub fn ensure_builtin_lists(&mut self) -> Result<()> {
        let now = current_ts();
        let defaults = [("Избранное", 0i64), ("К прочтению", 1i64)];
        for (i, (name, _)) in defaults.iter().enumerate() {
            self.conn.execute(
                "INSERT OR IGNORE INTO lists(name, builtin, position, created_at)
                 VALUES(?1, 1, ?2, ?3)",
                params![name, i as i64, now],
            )?;
        }
        Ok(())
    }

    pub fn list_lists(&self) -> Result<Vec<UserList>> {
        let mut stmt = self.conn.prepare(
            "SELECT l.id, l.name, l.builtin,
                    (SELECT COUNT(*) FROM list_items WHERE list_id = l.id) AS cnt
             FROM lists l
             ORDER BY l.builtin DESC, l.position, l.name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(UserList {
                id: row.get(0)?,
                name: row.get(1)?,
                builtin: row.get::<_, i64>(2)? != 0,
                item_count: row.get::<_, i64>(3)? as u32,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_list(&self, id: i64) -> Result<Option<UserList>> {
        Ok(self
            .conn
            .query_row(
                "SELECT l.id, l.name, l.builtin,
                        (SELECT COUNT(*) FROM list_items WHERE list_id = l.id)
                 FROM lists l WHERE l.id = ?1",
                params![id],
                |row| {
                    Ok(UserList {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        builtin: row.get::<_, i64>(2)? != 0,
                        item_count: row.get::<_, i64>(3)? as u32,
                    })
                },
            )
            .optional()?)
    }

    pub fn create_list(&mut self, name: &str) -> Result<UserList> {
        let trimmed = name.trim();
        if trimmed.is_empty() {
            return Err(Error::Other("название списка пустое".into()));
        }
        self.conn.execute(
            "INSERT INTO lists(name, builtin, position, created_at)
             VALUES(?1, 0, (SELECT COALESCE(MAX(position), 0) + 1 FROM lists), ?2)",
            params![trimmed, current_ts()],
        )?;
        let id = self.conn.last_insert_rowid();
        Ok(UserList {
            id,
            name: trimmed.to_string(),
            builtin: false,
            item_count: 0,
        })
    }

    pub fn rename_list(&mut self, id: i64, new_name: &str) -> Result<()> {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err(Error::Other("название списка пустое".into()));
        }
        let builtin: i64 = self
            .conn
            .query_row("SELECT builtin FROM lists WHERE id = ?1", params![id], |r| {
                r.get(0)
            })?;
        if builtin != 0 {
            return Err(Error::Other(
                "встроенный список нельзя переименовать".into(),
            ));
        }
        self.conn.execute(
            "UPDATE lists SET name = ?1 WHERE id = ?2",
            params![trimmed, id],
        )?;
        Ok(())
    }

    pub fn delete_list(&mut self, id: i64) -> Result<()> {
        let builtin: i64 = self
            .conn
            .query_row("SELECT builtin FROM lists WHERE id = ?1", params![id], |r| {
                r.get(0)
            })?;
        if builtin != 0 {
            return Err(Error::Other("встроенный список нельзя удалить".into()));
        }
        self.conn
            .execute("DELETE FROM lists WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn add_to_list(&mut self, list_id: i64, kind: &str, ref_key: &str) -> Result<()> {
        validate_kind(kind)?;
        if ref_key.is_empty() {
            return Err(Error::Other("пустой ключ для списка".into()));
        }
        self.conn.execute(
            "INSERT OR IGNORE INTO list_items(list_id, kind, ref_key, added_at)
             VALUES(?1, ?2, ?3, ?4)",
            params![list_id, kind, ref_key, current_ts()],
        )?;
        Ok(())
    }

    pub fn remove_from_list(&mut self, list_id: i64, kind: &str, ref_key: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM list_items WHERE list_id = ?1 AND kind = ?2 AND ref_key = ?3",
            params![list_id, kind, ref_key],
        )?;
        Ok(())
    }

    /// Return lists that already contain the given ref. Useful for showing
    /// which lists an item is already in, so the menu can render a check.
    pub fn lists_containing(&self, kind: &str, ref_key: &str) -> Result<Vec<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT list_id FROM list_items WHERE kind = ?1 AND ref_key = ?2")?;
        let rows = stmt.query_map(params![kind, ref_key], |r| r.get::<_, i64>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn list_contents(&self, list_id: i64) -> Result<ListContents> {
        let list = self
            .get_list(list_id)?
            .ok_or_else(|| Error::NotFound(format!("список {list_id}")))?;

        let mut stmt = self.conn.prepare(
            "SELECT kind, ref_key FROM list_items WHERE list_id = ?1 ORDER BY added_at DESC",
        )?;
        let rows = stmt.query_map(params![list_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut book_keys = Vec::new();
        let mut author_keys = Vec::new();
        let mut series_keys = Vec::new();
        for r in rows {
            let (kind, key) = r?;
            match kind.as_str() {
                "book" => book_keys.push(key),
                "author" => author_keys.push(key),
                "series" => series_keys.push(key),
                _ => {}
            }
        }

        // Resolve books by lib_id (preferred) or "archive|file" (fallback).
        let mut books: Vec<BookListItem> = Vec::new();
        let mut orphans: Vec<OrphanItem> = Vec::new();
        let book_sql = format!(
            "SELECT {COLS} FROM books b
             WHERE b.deleted = 0 AND (b.lib_id = ?1 OR (b.archive || '|' || b.file) = ?1)
             LIMIT 1"
        );
        let mut find_book = self.conn.prepare(&book_sql)?;
        for key in &book_keys {
            let row: Option<BookListItem> = find_book
                .query_row(params![key], map_book_row)
                .optional()?;
            match row {
                Some(b) => books.push(b),
                None => orphans.push(OrphanItem {
                    kind: "book".into(),
                    ref_key: key.clone(),
                }),
            }
        }

        // Authors by display name; book count uses current catalog.
        let mut authors: Vec<AuthorHit> = Vec::new();
        let mut find_author = self.conn.prepare(
            "SELECT a.id, a.display,
                    (SELECT COUNT(*) FROM book_authors ba JOIN books b ON b.id = ba.book_id
                     WHERE ba.author_id = a.id AND b.deleted = 0) AS cnt
             FROM authors a WHERE a.display = ?1",
        )?;
        for key in &author_keys {
            let row: Option<AuthorHit> = find_author
                .query_row(params![key], |row| {
                    Ok(AuthorHit {
                        id: row.get(0)?,
                        display: row.get(1)?,
                        book_count: row.get::<_, i64>(2)? as u32,
                    })
                })
                .optional()?;
            match row {
                Some(a) => authors.push(a),
                None => orphans.push(OrphanItem {
                    kind: "author".into(),
                    ref_key: key.clone(),
                }),
            }
        }

        // Series: just name + current count
        let mut series: Vec<SeriesHit> = Vec::new();
        let mut count_series = self.conn.prepare(
            "SELECT COUNT(*) FROM books WHERE series = ?1 AND deleted = 0",
        )?;
        for key in &series_keys {
            let cnt: i64 = count_series
                .query_row(params![key], |r| r.get(0))
                .unwrap_or(0);
            if cnt == 0 {
                orphans.push(OrphanItem {
                    kind: "series".into(),
                    ref_key: key.clone(),
                });
            } else {
                series.push(SeriesHit {
                    name: key.clone(),
                    book_count: cnt as u32,
                });
            }
        }

        Ok(ListContents {
            list,
            books,
            authors,
            series,
            orphans,
        })
    }

    // ---- Reading progress -------------------------------------------------

    pub fn get_reading_position(
        &self,
        lib_id: &str,
    ) -> Result<Option<crate::model::ReadingPosition>> {
        Ok(self
            .conn
            .query_row(
                "SELECT lib_id, chapter_id, scroll, updated_at
                 FROM reading_progress WHERE lib_id = ?1",
                params![lib_id],
                |r| {
                    Ok(crate::model::ReadingPosition {
                        lib_id: r.get(0)?,
                        chapter_id: r.get(1)?,
                        scroll: r.get(2)?,
                        updated_at: r.get(3)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn save_reading_position(
        &mut self,
        lib_id: &str,
        chapter_id: &str,
        scroll: f64,
    ) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        self.conn.execute(
            "INSERT INTO reading_progress(lib_id, chapter_id, scroll, updated_at)
             VALUES(?1, ?2, ?3, ?4)
             ON CONFLICT(lib_id) DO UPDATE SET
               chapter_id = excluded.chapter_id,
               scroll     = excluded.scroll,
               updated_at = excluded.updated_at",
            params![lib_id, chapter_id, scroll, now],
        )?;
        Ok(())
    }

    // ---- External book metadata (Google Books / OpenLibrary) -------------

    pub fn get_external_meta(
        &self,
        lib_id: &str,
    ) -> Result<Vec<crate::model::ExternalMetaEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT source, status, description, rating, rating_count, url, fetched_at
             FROM book_external_meta WHERE lib_id = ?1
             ORDER BY source",
        )?;
        let rows = stmt.query_map(params![lib_id], |r| {
            Ok(crate::model::ExternalMetaEntry {
                source: r.get(0)?,
                status: r.get(1)?,
                description: r.get(2)?,
                rating: r.get(3)?,
                rating_count: r.get(4)?,
                url: r.get(5)?,
                fetched_at: r.get(6)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn put_external_meta(
        &mut self,
        lib_id: &str,
        entry: &crate::model::ExternalMetaEntry,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO book_external_meta(lib_id, source, status, description, rating, rating_count, url, fetched_at)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(lib_id, source) DO UPDATE SET
               status       = excluded.status,
               description  = excluded.description,
               rating       = excluded.rating,
               rating_count = excluded.rating_count,
               url          = excluded.url,
               fetched_at   = excluded.fetched_at",
            params![
                lib_id,
                entry.source,
                entry.status,
                entry.description,
                entry.rating,
                entry.rating_count,
                entry.url,
                entry.fetched_at,
            ],
        )?;
        Ok(())
    }
}

/// Column list shared by every query that returns a `BookListItem`. The order
/// must match `map_book_row`.
const COLS: &str = "b.id,
       COALESCE(b.lib_id, '') AS lib_id,
       b.title,
       b.series,
       b.ser_no,
       b.lang,
       b.size,
       COALESCE((SELECT GROUP_CONCAT(a_.display, ', ')
                 FROM book_authors ba_ JOIN authors a_ ON a_.id = ba_.author_id
                 WHERE ba_.book_id = b.id), '') AS authors,
       b.ext";

fn map_book_row(row: &Row<'_>) -> rusqlite::Result<BookListItem> {
    Ok(BookListItem {
        id: row.get(0)?,
        lib_id: row.get::<_, String>(1)?,
        title: row.get(2)?,
        series: row.get::<_, Option<String>>(3)?,
        ser_no: row.get::<_, Option<u32>>(4)?,
        lang: row.get::<_, Option<String>>(5)?.unwrap_or_default(),
        size: row.get::<_, i64>(6)? as u64,
        authors: row.get::<_, String>(7)?,
        ext: row.get::<_, String>(8)?,
    })
}

fn validate_kind(kind: &str) -> Result<()> {
    match kind {
        "book" | "author" | "series" => Ok(()),
        _ => Err(Error::Other(format!("неизвестный тип элемента: {kind}"))),
    }
}

fn current_ts() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Register the scalar UDFs we use to bucket strings for the alphabet index.
/// `first_letter('(псевдоним) Иванов')` → `'И'`. `first_n_alpha(s, n)` returns
/// the first `n` alphabetic chars from `s` after skipping leading punctuation
/// and digits; the first char is uppercased, the rest lowercased so it folds
/// case-insensitively at the SQL `=` comparison level.
fn register_text_udfs(conn: &Connection) -> Result<()> {
    use rusqlite::functions::FunctionFlags;
    conn.create_scalar_function(
        "first_letter",
        1,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let s: String = ctx.get(0)?;
            Ok(first_letter_normalized(&s))
        },
    )?;
    conn.create_scalar_function(
        "first_n_alpha",
        2,
        FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
        |ctx| {
            let s: String = ctx.get(0)?;
            let n: i64 = ctx.get(1)?;
            Ok(first_n_alpha(&s, n.max(0) as usize))
        },
    )?;
    Ok(())
}

fn first_letter_normalized(s: &str) -> Option<String> {
    for ch in s.chars() {
        if ch.is_alphabetic() {
            let mut out = String::new();
            for u in ch.to_uppercase() {
                out.push(u);
            }
            return Some(out);
        }
    }
    None
}

fn first_n_alpha(s: &str, n: usize) -> Option<String> {
    if n == 0 {
        return None;
    }
    let mut started = false;
    let mut taken = 0usize;
    let mut out = String::new();
    for ch in s.chars() {
        if !started {
            if !ch.is_alphabetic() {
                continue;
            }
            started = true;
            for u in ch.to_uppercase() {
                out.push(u);
            }
            taken += 1;
            if taken >= n {
                break;
            }
        } else {
            for l in ch.to_lowercase() {
                out.push(l);
            }
            taken += 1;
            if taken >= n {
                break;
            }
        }
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Render `BookFilters` as a SQL `AND ...` fragment plus the positional values
/// to bind. Assumes the surrounding query already declares the `books` row as
/// alias `b`. Empty filters yield ("", []), keeping unfiltered queries cheap.
fn filter_sql(f: &BookFilters) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
    let mut sql = String::new();
    let mut vals: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(lang) = f.lang.as_ref().filter(|s| !s.is_empty()) {
        sql.push_str(" AND b.lang = ?");
        vals.push(Box::new(lang.clone()));
    }
    if let Some(archive) = f.archive.as_ref().filter(|s| !s.is_empty()) {
        sql.push_str(" AND b.archive = ?");
        vals.push(Box::new(archive.clone()));
    }
    if let Some(genre) = f.genre.as_ref().filter(|s| !s.is_empty()) {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM book_genres bg \
              JOIN genres g ON g.id = bg.genre_id \
              WHERE bg.book_id = b.id AND g.code = ?)",
        );
        vals.push(Box::new(genre.clone()));
    }
    if let Some(author_id) = f.author_id {
        sql.push_str(
            " AND EXISTS (SELECT 1 FROM book_authors ba \
              WHERE ba.book_id = b.id AND ba.author_id = ?)",
        );
        vals.push(Box::new(author_id));
    }
    (sql, vals)
}

/// Translate a free-form search query into an FTS5 MATCH expression that
/// tolerates partial words. Each non-empty token becomes a bareword prefix
/// match — we strip non-alphanumeric chars first so the result is safe to
/// splice without any quoting (FTS5 reserved chars never leak through).
fn build_fts_query(q: &str) -> String {
    q.split_whitespace()
        .filter(|t| !t.is_empty())
        .map(|t| {
            let cleaned: String = t
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect();
            if cleaned.is_empty() {
                String::new()
            } else {
                format!("{cleaned}*")
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rec(title: &str, last: &str, first: &str) -> InpRecord {
        InpRecord {
            authors: vec![AuthorName {
                last: last.into(),
                first: first.into(),
                middle: String::new(),
            }],
            genres: vec!["test".into()],
            title: title.into(),
            file: "1".into(),
            archive: "a.zip".into(),
            size: 1,
            ext: "fb2".into(),
            ..Default::default()
        }
    }

    #[test]
    fn import_search_and_fetch() {
        let mut idx = LibraryIndex::open(":memory:").unwrap();
        idx.import_batch(&[
            rec("The Hobbit", "Tolkien", "John"),
            rec("Dune", "Herbert", "Frank"),
        ])
        .unwrap();

        let empty = BookFilters::default();
        let all = idx.list_books(None, &empty, 100, 0).unwrap();
        assert_eq!(all.len(), 2);

        let hits = idx.list_books(Some("hob"), &empty, 100, 0).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "The Hobbit");

        let by_author = idx.list_books(Some("Herbert"), &empty, 100, 0).unwrap();
        assert_eq!(by_author.len(), 1);
        assert!(by_author[0].authors.contains("Herbert"));

        let book = idx.get_book(hits[0].id).unwrap().unwrap();
        assert_eq!(book.authors.len(), 1);
        assert_eq!(book.authors[0].last, "Tolkien");
        assert_eq!(book.genres, vec!["test"]);

        // Direct author search via FTS
        let author_hits = idx.search_authors("Tolk", &empty, 10).unwrap();
        assert_eq!(author_hits.len(), 1);
        assert_eq!(author_hits[0].display, "Tolkien John");
        assert_eq!(author_hits[0].book_count, 1);

        // Series search
        let series_hits = idx.search_series("hob", &empty, 10).unwrap();
        assert_eq!(series_hits.len(), 0); // no series in test data
    }

    #[test]
    fn user_list_count_updates_after_add() {
        let mut idx = LibraryIndex::open(":memory:").unwrap();
        idx.ensure_builtin_lists().unwrap();
        let custom = idx.create_list("Тестовый список").unwrap();
        assert_eq!(custom.item_count, 0);

        idx.add_to_list(custom.id, "author", "Толстой Лев").unwrap();
        idx.add_to_list(custom.id, "book", "FB-001").unwrap();
        // duplicate should be ignored
        idx.add_to_list(custom.id, "author", "Толстой Лев").unwrap();

        let all = idx.list_lists().unwrap();
        let same = all.iter().find(|l| l.id == custom.id).unwrap();
        assert_eq!(same.item_count, 2, "INSERT OR IGNORE should de-dup");

        idx.remove_from_list(custom.id, "book", "FB-001").unwrap();
        let all = idx.list_lists().unwrap();
        let same = all.iter().find(|l| l.id == custom.id).unwrap();
        assert_eq!(same.item_count, 1);
    }
}
