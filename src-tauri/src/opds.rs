use std::io::Cursor;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpListener};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use tiny_http::{Header, Method, Response, Server};

use crate::error::{Error, Result};
use crate::library::LibraryState;
use crate::model::{AuthorHit, BookFilters, BookListItem, SeriesHit};

const PAGE_SIZE: i64 = 60;

pub struct OpdsServer {
    addr: SocketAddr,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl OpdsServer {
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// Bind a TCP listener to 127.0.0.1 on an ephemeral port and spawn the
    /// server thread. Stays bound to localhost — external access is only via
    /// the ngrok tunnel.
    pub fn start(state: Arc<LibraryState>) -> Result<Self> {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0))
            .map_err(|e| Error::Other(format!("opds bind: {e}")))?;
        let addr = listener
            .local_addr()
            .map_err(|e| Error::Other(format!("opds addr: {e}")))?;
        let server = Server::from_listener(listener, None)
            .map_err(|e| Error::Other(format!("opds server: {e}")))?;

        let stop = Arc::new(AtomicBool::new(false));
        let stop_clone = Arc::clone(&stop);
        let state_clone = Arc::clone(&state);
        let handle = thread::Builder::new()
            .name("opds-server".into())
            .spawn(move || run_loop(server, state_clone, stop_clone))
            .map_err(|e| Error::Other(format!("opds spawn: {e}")))?;

        Ok(Self {
            addr,
            stop,
            handle: Some(handle),
        })
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(h) = self.handle.take() {
            // Best-effort: poke ourselves so recv_timeout returns immediately.
            let _ = std::net::TcpStream::connect(self.addr);
            let _ = h.join();
        }
    }
}

fn run_loop(server: Server, state: Arc<LibraryState>, stop: Arc<AtomicBool>) {
    while !stop.load(Ordering::SeqCst) {
        match server.recv_timeout(Duration::from_millis(250)) {
            Ok(Some(req)) => {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                handle_request(req, &state);
            }
            Ok(None) => continue,
            Err(_) => break,
        }
    }
}

fn handle_request(req: tiny_http::Request, state: &Arc<LibraryState>) {
    if !matches!(req.method(), Method::Get | Method::Head) {
        let _ = req.respond(text_response(405, "method not allowed").into_http());
        return;
    }
    let url = req.url().to_string();
    let (path, query) = split_query(&url);
    tracing::debug!(target = "opds", path = %path, query = %query, "request");

    let result = match path.as_str() {
        "/" | "/opds" | "/opds/" => Ok(root_feed(state)),
        "/opds/languages" => Ok(languages_feed(state)),
        "/opds/recent" => books_feed(state, &query, FeedKind::Recent),
        "/opds/authors" => authors_feed(state, &query),
        "/opds/series" => series_feed(state, &query),

        // OpenSearch descriptors per scope.
        "/opds/search.xml" | "/opds/search/books.xml" => Ok(opensearch("books")),
        "/opds/search/authors.xml" => Ok(opensearch("authors")),
        "/opds/search/series.xml" => Ok(opensearch("series")),

        // Search root: gives the client three search entry points.
        "/opds/search-root" => Ok(search_root_feed()),

        // Typed search endpoints.
        "/opds/search" | "/opds/search/books" => books_feed(state, &query, FeedKind::Search),
        "/opds/search/authors" => search_authors_feed(state, &query),
        "/opds/search/series" => search_series_feed(state, &query),

        // Browse: language → letter → 2-letter prefix → authors.
        p if p.starts_with("/opds/lang/") => browse_by_path(state, p, &query),

        p if p.starts_with("/opds/author/") => {
            let id: i64 = p["/opds/author/".len()..].parse().unwrap_or(-1);
            books_feed(state, &query, FeedKind::Author(id))
        }
        p if p.starts_with("/opds/series/") => {
            let name = urldecode(&p["/opds/series/".len()..]);
            books_feed(state, &query, FeedKind::Series(name))
        }
        p if p.starts_with("/opds/book/") => {
            let id: i64 = p["/opds/book/".len()..].parse().unwrap_or(-1);
            book_entry_feed(state, id)
        }
        p if p.starts_with("/opds/cover/") => {
            let id: i64 = p["/opds/cover/".len()..].parse().unwrap_or(-1);
            return serve_cover(req, state, id);
        }
        p if p.starts_with("/opds/download/") => {
            let tail = &p["/opds/download/".len()..];
            // Accept either `/opds/download/123` (native bytes) or
            // `/opds/download/123.epub` (transcode FB2→EPUB).
            let (id_str, as_epub) = match tail.strip_suffix(".epub") {
                Some(s) => (s, true),
                None => (tail, false),
            };
            let id: i64 = id_str.parse().unwrap_or(-1);
            return serve_download(req, state, id, as_epub);
        }
        _ => Ok(text_response(404, "not found")),
    };

    match result {
        Ok(resp) => {
            let _ = req.respond(resp.into_http());
        }
        Err(e) => {
            tracing::warn!(target = "opds", error = %e, "request failed");
            let _ = req.respond(text_response(500, &format!("{e}")).into_http());
        }
    }
}

// ---------- routing helpers --------------------------------------------------

enum FeedKind {
    Recent,
    Author(i64),
    Series(String),
    Search,
}

struct AtomResponse {
    bytes: Vec<u8>,
    content_type: &'static str,
    status: u16,
}

impl AtomResponse {
    fn xml(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            content_type: "application/atom+xml;profile=opds-catalog;charset=utf-8",
            status: 200,
        }
    }
    fn opensearch(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            content_type: "application/opensearchdescription+xml;charset=utf-8",
            status: 200,
        }
    }
    fn into_http(self) -> Response<Cursor<Vec<u8>>> {
        let len = self.bytes.len();
        Response::new(
            self.status.into(),
            vec![header("Content-Type", self.content_type)],
            Cursor::new(self.bytes),
            Some(len),
            None,
        )
    }
}

fn text_response(status: u16, msg: &str) -> AtomResponse {
    AtomResponse {
        bytes: msg.as_bytes().to_vec(),
        content_type: "text/plain;charset=utf-8",
        status,
    }
}

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).expect("valid header")
}

fn split_query(url: &str) -> (String, String) {
    match url.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (url.to_string(), String::new()),
    }
}

fn urldecode(s: &str) -> String {
    urlencoding::decode(s)
        .map(|c| c.into_owned())
        .unwrap_or_else(|_| s.to_string())
}

fn query_param(q: &str, key: &str) -> Option<String> {
    for pair in q.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some(kv) => kv,
            None => (pair, ""),
        };
        if k == key {
            return Some(urldecode(&v.replace('+', " ")));
        }
    }
    None
}

fn query_param_i64(q: &str, key: &str, default: i64) -> i64 {
    query_param(q, key)
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(default)
}

// ---------- feeds ------------------------------------------------------------

fn root_feed(state: &Arc<LibraryState>) -> AtomResponse {
    let stats = state.stats().unwrap_or_default();
    let collection = state.get_meta("collection_name").ok().flatten();
    let title = collection.unwrap_or_else(|| "MyLib".to_string());

    let mut x = XmlBuf::new();
    x.feed_start(&title, "/opds");
    x.link_search_books();
    x.nav_entry(
        "По языкам",
        "/opds/languages",
        "Каталог по языкам и алфавиту",
    );
    x.nav_entry(
        "Последние книги",
        "/opds/recent",
        &format!("Все книги ({})", stats.books),
    );
    x.nav_entry(
        "Поиск",
        "/opds/search-root",
        "Поиск по авторам, сериям, книгам",
    );
    x.nav_entry(
        "Все авторы",
        "/opds/authors",
        &format!("Полный список авторов ({})", stats.authors),
    );
    x.nav_entry(
        "Все серии",
        "/opds/series",
        &format!("Полный список серий ({})", stats.series),
    );
    x.feed_end();
    AtomResponse::xml(x.into_bytes())
}

fn languages_feed(state: &Arc<LibraryState>) -> AtomResponse {
    let langs = state.list_languages(&BookFilters::default()).unwrap_or_default();
    let mut x = XmlBuf::new();
    x.feed_start("Языки", "/opds/languages");
    if langs.is_empty() {
        x.nav_entry("Все языки", "/opds/lang/_all", "Каталог независимо от языка");
    } else {
        for l in &langs {
            let label = format!("{} ({} книг)", language_label(&l.code), l.count);
            x.nav_entry(&label, &format!("/opds/lang/{}", urlencoding::encode(&l.code)), "");
        }
        x.nav_entry(
            "Все языки",
            "/opds/lang/_all",
            "Без фильтра по языку",
        );
    }
    x.feed_end();
    AtomResponse::xml(x.into_bytes())
}

fn search_root_feed() -> AtomResponse {
    let mut x = XmlBuf::new();
    x.feed_start("Поиск", "/opds/search-root");
    x.nav_entry_with_search(
        "Поиск книг",
        "/opds/search/books",
        "/opds/search/books.xml",
        "Поиск по названию, автору, серии",
    );
    x.nav_entry_with_search(
        "Поиск авторов",
        "/opds/search/authors",
        "/opds/search/authors.xml",
        "Поиск по ФИО автора",
    );
    x.nav_entry_with_search(
        "Поиск серий",
        "/opds/search/series",
        "/opds/search/series.xml",
        "Поиск по названию серии",
    );
    x.feed_end();
    AtomResponse::xml(x.into_bytes())
}

/// Per-scope OpenSearch description. The `scope` value (`books`/`authors`/
/// `series`) determines which endpoint the client hits with the query.
fn opensearch(scope: &str) -> AtomResponse {
    let (short, descr, template) = match scope {
        "authors" => (
            "MyLib — авторы",
            "Поиск авторов по библиотеке MyLib",
            "/opds/search/authors?q={searchTerms}",
        ),
        "series" => (
            "MyLib — серии",
            "Поиск серий по библиотеке MyLib",
            "/opds/search/series?q={searchTerms}",
        ),
        _ => (
            "MyLib — книги",
            "Поиск книг по библиотеке MyLib",
            "/opds/search/books?q={searchTerms}",
        ),
    };
    let body = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
  <ShortName>{short}</ShortName>
  <Description>{descr}</Description>
  <InputEncoding>UTF-8</InputEncoding>
  <Url type="application/atom+xml;profile=opds-catalog" template="{template}"/>
</OpenSearchDescription>
"#,
        short = escape(short),
        descr = escape(descr),
        template = escape(template),
    );
    AtomResponse::opensearch(body.into_bytes())
}

fn books_feed(state: &Arc<LibraryState>, query: &str, kind: FeedKind) -> Result<AtomResponse> {
    let offset = query_param_i64(query, "offset", 0).max(0);
    let mut x = XmlBuf::new();

    match kind {
        FeedKind::Recent => {
            let rows = state.list_books(None, &BookFilters::default(), PAGE_SIZE, offset)?;
            x.feed_start("Последние книги", "/opds/recent");
            x.link_search_books();
            push_book_entries(&mut x, &rows);
            paginate(&mut x, "/opds/recent", &rows, offset);
        }
        FeedKind::Author(id) => {
            let lang = query_param(query, "lang");
            let lang_str = lang.as_deref().filter(|s| !s.is_empty());
            let display = state.author_display(id)?.unwrap_or_else(|| format!("Автор #{id}"));
            let filters = BookFilters {
                lang: lang_str.map(|s| s.to_string()),
                ..Default::default()
            };
            let rows = state.books_by_author(id, &filters)?;
            let title = format!("Книги: {display}");
            let self_link = match lang_str {
                Some(l) => format!("/opds/author/{id}?lang={}", urlencoding::encode(l)),
                None => format!("/opds/author/{id}"),
            };
            x.feed_start(&title, &self_link);
            push_book_entries(&mut x, &rows);
        }
        FeedKind::Series(name) => {
            let lang = query_param(query, "lang");
            let lang_str = lang.as_deref().filter(|s| !s.is_empty());
            let filters = BookFilters {
                lang: lang_str.map(|s| s.to_string()),
                ..Default::default()
            };
            let rows = state.series_view(&name, &filters)?;
            let title = format!("Серия: {name}");
            let enc = urlencoding::encode(&name);
            x.feed_start(&title, &format!("/opds/series/{enc}"));
            push_book_entries(&mut x, &rows);
        }
        FeedKind::Search => {
            let q = query_param(query, "q").unwrap_or_default();
            let q_trim = q.trim();
            let mut rows: Vec<BookListItem> = Vec::new();
            if !q_trim.is_empty() {
                rows = state.list_books(
                    Some(q_trim),
                    &BookFilters::default(),
                    PAGE_SIZE,
                    offset,
                )?;
            }
            let title = if q_trim.is_empty() {
                "Поиск".to_string()
            } else {
                format!("Поиск: {q_trim}")
            };
            let self_link = format!("/opds/search?q={}", urlencoding::encode(q_trim));
            x.feed_start(&title, &self_link);
            push_book_entries(&mut x, &rows);
            if !q_trim.is_empty() {
                paginate(&mut x, &self_link, &rows, offset);
            }
        }
    }

    x.feed_end();
    Ok(AtomResponse::xml(x.into_bytes()))
}

// ---------- alphabet browsing -----------------------------------------------

/// Dispatches `/opds/lang/{code}[/letter/{X}|/prefix/{XX}]` to the right feed.
fn browse_by_path(
    state: &Arc<LibraryState>,
    path: &str,
    _query: &str,
) -> Result<AtomResponse> {
    // path starts with /opds/lang/
    let tail = &path["/opds/lang/".len()..];
    let parts: Vec<&str> = tail.split('/').filter(|s| !s.is_empty()).collect();

    match parts.as_slice() {
        // /opds/lang/{code}
        [code] => {
            let lang_code = urldecode(code);
            Ok(letters_feed(state, &lang_code))
        }
        // /opds/lang/{code}/letter/{X}
        [code, "letter", letter] => {
            let lang_code = urldecode(code);
            let letter = urldecode(letter);
            Ok(prefixes_feed(state, &lang_code, &letter))
        }
        // /opds/lang/{code}/prefix/{XX}
        [code, "prefix", prefix] => {
            let lang_code = urldecode(code);
            let prefix = urldecode(prefix);
            Ok(authors_by_prefix_feed(state, &lang_code, &prefix))
        }
        _ => Ok(text_response(404, "not found")),
    }
}

fn lang_filter(code: &str) -> Option<&str> {
    if code == "_all" {
        None
    } else {
        Some(code)
    }
}

fn lang_filters(code: &str) -> BookFilters {
    BookFilters {
        lang: lang_filter(code).map(|s| s.to_string()),
        ..Default::default()
    }
}

fn lang_segment(code: &str) -> String {
    if code == "_all" {
        "_all".to_string()
    } else {
        urlencoding::encode(code).into_owned()
    }
}

fn letters_feed(state: &Arc<LibraryState>, code: &str) -> AtomResponse {
    let filters = lang_filters(code);
    let letters = state.author_first_letters(&filters).unwrap_or_default();
    let title = format!("Авторы — {}", language_label(code));
    let self_link = format!("/opds/lang/{}", lang_segment(code));

    let mut x = XmlBuf::new();
    x.feed_start(&title, &self_link);
    x.link_search_books();
    if letters.is_empty() {
        x.nav_entry(
            "Нет авторов в этом языке",
            &self_link,
            "Попробуйте другой язык в каталоге",
        );
    }
    for (lt, cnt) in &letters {
        let label = format!("{lt} ({cnt})");
        let href = format!(
            "/opds/lang/{}/letter/{}",
            lang_segment(code),
            urlencoding::encode(lt)
        );
        x.nav_entry(&label, &href, "");
    }
    x.feed_end();
    AtomResponse::xml(x.into_bytes())
}

fn prefixes_feed(state: &Arc<LibraryState>, code: &str, letter: &str) -> AtomResponse {
    let filters = lang_filters(code);
    let prefixes = state
        .author_two_letter_prefixes(&filters, letter)
        .unwrap_or_default();
    let title = format!("«{letter}» — {}", language_label(code));
    let self_link = format!(
        "/opds/lang/{}/letter/{}",
        lang_segment(code),
        urlencoding::encode(letter)
    );

    let mut x = XmlBuf::new();
    x.feed_start(&title, &self_link);
    // Always offer single-letter prefix as a shortcut to all authors starting
    // with this letter, in case the user just wants a flat list.
    let single = format!(
        "/opds/lang/{}/prefix/{}",
        lang_segment(code),
        urlencoding::encode(letter)
    );
    x.nav_entry(
        &format!("Все на «{letter}…»"),
        &single,
        "Авторы без разбивки по второй букве",
    );
    for (pfx, cnt) in &prefixes {
        // Display: capital + lowercase second letter for tidy "Аа/Аб/Ав" feel.
        let display = pretty_prefix(pfx);
        let label = format!("{display} ({cnt})");
        let href = format!(
            "/opds/lang/{}/prefix/{}",
            lang_segment(code),
            urlencoding::encode(pfx)
        );
        x.nav_entry(&label, &href, "");
    }
    x.feed_end();
    AtomResponse::xml(x.into_bytes())
}

fn authors_by_prefix_feed(state: &Arc<LibraryState>, code: &str, prefix: &str) -> AtomResponse {
    let filters = lang_filters(code);
    let authors = state.authors_by_prefix(&filters, prefix).unwrap_or_default();
    let title = format!("Авторы — {} ({})", pretty_prefix(prefix), language_label(code));
    let self_link = format!(
        "/opds/lang/{}/prefix/{}",
        lang_segment(code),
        urlencoding::encode(prefix)
    );

    let mut x = XmlBuf::new();
    x.feed_start(&title, &self_link);
    if authors.is_empty() {
        x.nav_entry("Нет авторов", &self_link, "");
    }
    for a in &authors {
        let label = if a.book_count > 0 {
            format!("{} ({})", a.display, a.book_count)
        } else {
            a.display.clone()
        };
        let href = if let Some(l) = filters.lang.as_deref() {
            format!(
                "/opds/author/{}?lang={}",
                a.id,
                urlencoding::encode(l)
            )
        } else {
            format!("/opds/author/{}", a.id)
        };
        x.nav_entry(&label, &href, "");
    }
    x.feed_end();
    AtomResponse::xml(x.into_bytes())
}

fn mime_for_ext(ext: &str) -> &'static str {
    match ext.to_ascii_lowercase().as_str() {
        "fb2" => "application/fb2+xml",
        "fb2.zip" | "fbz" => "application/fb2+zip",
        "epub" => "application/epub+zip",
        "pdf" => "application/pdf",
        "mobi" | "prc" => "application/x-mobipocket-ebook",
        "azw" | "azw3" | "kf8" => "application/vnd.amazon.ebook",
        "djvu" => "image/vnd.djvu",
        "cbz" => "application/x-cbz",
        "cbr" => "application/x-cbr",
        "txt" => "text/plain",
        "rtf" => "application/rtf",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

fn pretty_prefix(s: &str) -> String {
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(a), Some(b)) => {
            // Keep first as-is, lowercase the second one.
            let mut out = String::new();
            out.push(a);
            for lc in b.to_lowercase() {
                out.push(lc);
            }
            out
        }
        _ => s.to_string(),
    }
}

fn language_label(code: &str) -> String {
    if code == "_all" {
        return "все языки".to_string();
    }
    match code.to_ascii_lowercase().as_str() {
        "ru" => "Русский".to_string(),
        "en" => "English".to_string(),
        "uk" | "ua" => "Українська".to_string(),
        "be" => "Беларуская".to_string(),
        "de" => "Deutsch".to_string(),
        "fr" => "Français".to_string(),
        "es" => "Español".to_string(),
        "it" => "Italiano".to_string(),
        "pl" => "Polski".to_string(),
        "kk" => "Қазақша".to_string(),
        "" => "—".to_string(),
        _ => code.to_string(),
    }
}

// ---------- typed search feeds ----------------------------------------------

fn search_authors_feed(state: &Arc<LibraryState>, query: &str) -> Result<AtomResponse> {
    let q = query_param(query, "q").unwrap_or_default();
    let q_trim = q.trim();
    let self_link = format!(
        "/opds/search/authors?q={}",
        urlencoding::encode(q_trim)
    );
    let mut x = XmlBuf::new();
    x.feed_start(
        &format!("Поиск авторов: {}", if q_trim.is_empty() { "—" } else { q_trim }),
        &self_link,
    );
    x.link_search_authors();
    if !q_trim.is_empty() {
        let rows = state.search_authors(q_trim, &BookFilters::default(), PAGE_SIZE)?;
        if rows.is_empty() {
            x.nav_entry("Ничего не найдено", &self_link, "");
        }
        for a in &rows {
            let label = if a.book_count > 0 {
                format!("{} ({})", a.display, a.book_count)
            } else {
                a.display.clone()
            };
            x.nav_entry(&label, &format!("/opds/author/{}", a.id), "");
        }
    }
    x.feed_end();
    Ok(AtomResponse::xml(x.into_bytes()))
}

fn search_series_feed(state: &Arc<LibraryState>, query: &str) -> Result<AtomResponse> {
    let q = query_param(query, "q").unwrap_or_default();
    let q_trim = q.trim();
    let self_link = format!(
        "/opds/search/series?q={}",
        urlencoding::encode(q_trim)
    );
    let mut x = XmlBuf::new();
    x.feed_start(
        &format!("Поиск серий: {}", if q_trim.is_empty() { "—" } else { q_trim }),
        &self_link,
    );
    x.link_search_series();
    if !q_trim.is_empty() {
        let rows = state.search_series(q_trim, &BookFilters::default(), PAGE_SIZE)?;
        if rows.is_empty() {
            x.nav_entry("Ничего не найдено", &self_link, "");
        }
        for s in &rows {
            let label = format!("{} ({})", s.name, s.book_count);
            let enc = urlencoding::encode(&s.name);
            x.nav_entry(&label, &format!("/opds/series/{enc}"), "");
        }
    }
    x.feed_end();
    Ok(AtomResponse::xml(x.into_bytes()))
}

fn authors_feed(state: &Arc<LibraryState>, query: &str) -> Result<AtomResponse> {
    let offset = query_param_i64(query, "offset", 0).max(0);
    let rows: Vec<AuthorHit> = state.list_authors(offset, PAGE_SIZE)?;
    let total = state.count_authors()?;

    let mut x = XmlBuf::new();
    x.feed_start(
        &format!("Авторы ({}–{} из {})", offset + 1, offset + rows.len() as i64, total),
        "/opds/authors",
    );
    x.link_search_authors();
    for a in &rows {
        let title = if a.book_count > 0 {
            format!("{} ({})", a.display, a.book_count)
        } else {
            a.display.clone()
        };
        x.nav_entry(&title, &format!("/opds/author/{}", a.id), "");
    }
    paginate_simple(&mut x, "/opds/authors", offset, rows.len() as i64, total);
    x.feed_end();
    Ok(AtomResponse::xml(x.into_bytes()))
}

fn series_feed(state: &Arc<LibraryState>, query: &str) -> Result<AtomResponse> {
    let offset = query_param_i64(query, "offset", 0).max(0);
    let rows: Vec<SeriesHit> = state.list_series(offset, PAGE_SIZE)?;
    let total = state.count_series()?;

    let mut x = XmlBuf::new();
    x.feed_start(
        &format!("Серии ({}–{} из {})", offset + 1, offset + rows.len() as i64, total),
        "/opds/series",
    );
    x.link_search_series();
    for s in &rows {
        let title = format!("{} ({})", s.name, s.book_count);
        let enc = urlencoding::encode(&s.name);
        x.nav_entry(&title, &format!("/opds/series/{enc}"), "");
    }
    paginate_simple(&mut x, "/opds/series", offset, rows.len() as i64, total);
    x.feed_end();
    Ok(AtomResponse::xml(x.into_bytes()))
}

fn book_entry_feed(state: &Arc<LibraryState>, id: i64) -> Result<AtomResponse> {
    let book = state
        .get_book(id)?
        .ok_or_else(|| Error::NotFound(format!("книга {id}")))?;
    let mut x = XmlBuf::new();
    x.feed_start(&book.title, &format!("/opds/book/{id}"));
    let item = BookListItem {
        id: book.id,
        lib_id: book.lib_id.clone(),
        title: book.title.clone(),
        authors: book
            .authors
            .iter()
            .map(|a| a.display())
            .collect::<Vec<_>>()
            .join(", "),
        series: book.series.clone(),
        ser_no: book.ser_no,
        lang: book.lang.clone(),
        size: book.size,
        ext: book.ext.clone(),
    };
    push_book_entries(&mut x, std::slice::from_ref(&item));
    x.feed_end();
    Ok(AtomResponse::xml(x.into_bytes()))
}

fn push_book_entries(x: &mut XmlBuf, rows: &[BookListItem]) {
    for b in rows {
        x.book_entry(b);
    }
}

fn paginate(x: &mut XmlBuf, base: &str, rows: &[BookListItem], offset: i64) {
    if rows.len() as i64 >= PAGE_SIZE {
        let next = offset + PAGE_SIZE;
        let sep = if base.contains('?') { '&' } else { '?' };
        let url = format!("{base}{sep}offset={next}");
        x.link(&url, "next", "application/atom+xml;profile=opds-catalog", "Дальше");
    }
    if offset > 0 {
        let prev = (offset - PAGE_SIZE).max(0);
        let sep = if base.contains('?') { '&' } else { '?' };
        let url = format!("{base}{sep}offset={prev}");
        x.link(&url, "previous", "application/atom+xml;profile=opds-catalog", "Назад");
    }
}

fn paginate_simple(x: &mut XmlBuf, base: &str, offset: i64, got: i64, total: i64) {
    if offset + got < total {
        let next = offset + PAGE_SIZE;
        x.link(
            &format!("{base}?offset={next}"),
            "next",
            "application/atom+xml;profile=opds-catalog",
            "Дальше",
        );
    }
    if offset > 0 {
        let prev = (offset - PAGE_SIZE).max(0);
        x.link(
            &format!("{base}?offset={prev}"),
            "previous",
            "application/atom+xml;profile=opds-catalog",
            "Назад",
        );
    }
}

// ---------- download ---------------------------------------------------------

fn serve_cover(req: tiny_http::Request, state: &Arc<LibraryState>, id: i64) {
    match state.read_book_cover(id) {
        Ok(Some((bytes, mime))) => {
            let len = bytes.len();
            let resp = Response::new(
                200u16.into(),
                vec![
                    header("Content-Type", &mime),
                    header("Cache-Control", "public, max-age=86400"),
                ],
                Cursor::new(bytes),
                Some(len),
                None,
            );
            let _ = req.respond(resp);
        }
        Ok(None) => {
            let _ = req.respond(text_response(404, "no cover").into_http());
        }
        Err(e) => {
            tracing::warn!(target = "opds", id, error = %e, "cover failed");
            let _ = req.respond(text_response(404, &format!("{e}")).into_http());
        }
    }
}

fn serve_download(req: tiny_http::Request, state: &Arc<LibraryState>, id: i64, as_epub: bool) {
    let res = if as_epub {
        state.read_book_as_epub(id)
    } else {
        state.read_book_bytes(id)
    };
    let (bytes, filename, ctype) = match res {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(target = "opds", id, as_epub, error = %e, "download failed");
            let _ = req.respond(text_response(404, &format!("{e}")).into_http());
            return;
        }
    };
    let len = bytes.len();
    let disposition = format!(
        "attachment; filename*=UTF-8''{}",
        urlencoding::encode(&filename)
    );
    let resp = Response::new(
        200u16.into(),
        vec![
            header("Content-Type", &ctype),
            header("Content-Disposition", &disposition),
            header("Cache-Control", "no-store"),
        ],
        Cursor::new(bytes),
        Some(len),
        None,
    );
    let _ = req.respond(resp);
}

// ---------- XML buffer -------------------------------------------------------

struct XmlBuf {
    out: String,
}

impl XmlBuf {
    fn new() -> Self {
        Self {
            out: String::with_capacity(4096),
        }
    }

    fn into_bytes(self) -> Vec<u8> {
        self.out.into_bytes()
    }

    fn feed_start(&mut self, title: &str, self_path: &str) {
        self.out.push_str(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom"
      xmlns:opds="http://opds-spec.org/2010/catalog"
      xmlns:dc="http://purl.org/dc/terms/">
"#,
        );
        self.out.push_str("  <id>tag:mylib:");
        self.out.push_str(&escape(self_path));
        self.out.push_str("</id>\n  <title>");
        self.out.push_str(&escape(title));
        self.out.push_str("</title>\n  <updated>");
        self.out.push_str(&now_iso8601());
        self.out.push_str("</updated>\n");
        self.link(self_path, "self", "application/atom+xml;profile=opds-catalog", "");
        self.link("/opds", "start", "application/atom+xml;profile=opds-catalog", "");
    }

    fn feed_end(&mut self) {
        self.out.push_str("</feed>\n");
    }

    fn link(&mut self, href: &str, rel: &str, ctype: &str, title: &str) {
        self.out.push_str("  <link href=\"");
        self.out.push_str(&escape(href));
        self.out.push_str("\" rel=\"");
        self.out.push_str(&escape(rel));
        self.out.push_str("\" type=\"");
        self.out.push_str(&escape(ctype));
        self.out.push('"');
        if !title.is_empty() {
            self.out.push_str(" title=\"");
            self.out.push_str(&escape(title));
            self.out.push('"');
        }
        self.out.push_str("/>\n");
    }

    fn link_search_books(&mut self) {
        self.link(
            "/opds/search/books.xml",
            "search",
            "application/opensearchdescription+xml",
            "",
        );
        self.link(
            "/opds/search/books?q={searchTerms}",
            "search",
            "application/atom+xml",
            "",
        );
    }

    fn link_search_authors(&mut self) {
        self.link(
            "/opds/search/authors.xml",
            "search",
            "application/opensearchdescription+xml",
            "",
        );
        self.link(
            "/opds/search/authors?q={searchTerms}",
            "search",
            "application/atom+xml",
            "",
        );
    }

    fn link_search_series(&mut self) {
        self.link(
            "/opds/search/series.xml",
            "search",
            "application/opensearchdescription+xml",
            "",
        );
        self.link(
            "/opds/search/series?q={searchTerms}",
            "search",
            "application/atom+xml",
            "",
        );
    }

    /// Like `nav_entry`, but attaches an OpenSearch `search` link to the entry
    /// itself so clients that focus the entry can pick up the right scope.
    fn nav_entry_with_search(
        &mut self,
        title: &str,
        href: &str,
        opensearch_href: &str,
        summary: &str,
    ) {
        self.out.push_str("  <entry>\n    <id>tag:mylib:");
        self.out.push_str(&escape(href));
        self.out.push_str("</id>\n    <title>");
        self.out.push_str(&escape(title));
        self.out.push_str("</title>\n    <updated>");
        self.out.push_str(&now_iso8601());
        self.out.push_str("</updated>\n");
        if !summary.is_empty() {
            self.out.push_str("    <content type=\"text\">");
            self.out.push_str(&escape(summary));
            self.out.push_str("</content>\n");
        }
        self.out.push_str("    <link href=\"");
        self.out.push_str(&escape(href));
        self.out
            .push_str("\" rel=\"subsection\" type=\"application/atom+xml;profile=opds-catalog\"/>\n");
        self.out.push_str("    <link href=\"");
        self.out.push_str(&escape(opensearch_href));
        self.out
            .push_str("\" rel=\"search\" type=\"application/opensearchdescription+xml\"/>\n");
        self.out.push_str("  </entry>\n");
    }

    fn nav_entry(&mut self, title: &str, href: &str, summary: &str) {
        self.out.push_str("  <entry>\n    <id>tag:mylib:");
        self.out.push_str(&escape(href));
        self.out.push_str("</id>\n    <title>");
        self.out.push_str(&escape(title));
        self.out.push_str("</title>\n    <updated>");
        self.out.push_str(&now_iso8601());
        self.out.push_str("</updated>\n");
        if !summary.is_empty() {
            self.out.push_str("    <content type=\"text\">");
            self.out.push_str(&escape(summary));
            self.out.push_str("</content>\n");
        }
        self.out.push_str("    <link href=\"");
        self.out.push_str(&escape(href));
        self.out
            .push_str("\" rel=\"subsection\" type=\"application/atom+xml;profile=opds-catalog\"/>\n");
        self.out.push_str("  </entry>\n");
    }

    fn book_entry(&mut self, b: &BookListItem) {
        let id_tag = format!("tag:mylib:book:{}", b.id);
        let book_href = format!("/opds/book/{}", b.id);
        let download_href = format!("/opds/download/{}", b.id);

        self.out.push_str("  <entry>\n    <id>");
        self.out.push_str(&escape(&id_tag));
        self.out.push_str("</id>\n    <title>");
        self.out.push_str(&escape(&b.title));
        self.out.push_str("</title>\n    <updated>");
        self.out.push_str(&now_iso8601());
        self.out.push_str("</updated>\n");

        if !b.authors.is_empty() {
            for name in b.authors.split(", ").filter(|s| !s.is_empty()) {
                self.out.push_str("    <author><name>");
                self.out.push_str(&escape(name));
                self.out.push_str("</name></author>\n");
            }
        }

        if let Some(s) = b.series.as_ref().filter(|s| !s.is_empty()) {
            let summary = match b.ser_no {
                Some(n) if n > 0 => format!("Серия: {s} #{n}"),
                _ => format!("Серия: {s}"),
            };
            self.out.push_str("    <content type=\"text\">");
            self.out.push_str(&escape(&summary));
            self.out.push_str("</content>\n");
        }

        if !b.lang.is_empty() {
            self.out.push_str("    <dc:language>");
            self.out.push_str(&escape(&b.lang));
            self.out.push_str("</dc:language>\n");
        }

        // Cover image — extracted from the FB2 inside the companion zip.
        // We advertise the same endpoint for both full and thumbnail; most
        // OPDS readers downscale client-side anyway.
        if b.ext.eq_ignore_ascii_case("fb2") {
            let cover_href = format!("/opds/cover/{}", b.id);
            self.out.push_str("    <link href=\"");
            self.out.push_str(&escape(&cover_href));
            self.out
                .push_str("\" rel=\"http://opds-spec.org/image\" type=\"image/jpeg\"/>\n");
            self.out.push_str("    <link href=\"");
            self.out.push_str(&escape(&cover_href));
            self.out
                .push_str("\" rel=\"http://opds-spec.org/image/thumbnail\" type=\"image/jpeg\"/>\n");
        }

        // Acquisition link — actual file download. We surface the real MIME
        // type so OPDS readers that filter their library by format (KyBook,
        // Marvin, FBReader) recognise the entry instead of hiding it.
        let mime = mime_for_ext(&b.ext);
        self.out.push_str("    <link href=\"");
        self.out.push_str(&escape(&download_href));
        self.out.push_str("\" rel=\"http://opds-spec.org/acquisition\" type=\"");
        self.out.push_str(&escape(mime));
        if b.size > 0 {
            self.out.push_str("\" length=\"");
            self.out.push_str(&b.size.to_string());
        }
        self.out.push_str("\"/>\n");

        // FB2 books also get an EPUB acquisition link — generated on the fly
        // by `/opds/download/{id}.epub`. Readers like KyBook that filter by
        // MIME will pick the EPUB variant; native FB2 readers can pick the
        // original. We deliberately don't advertise `length` here since the
        // transcoded size isn't known until conversion runs.
        if b.ext.eq_ignore_ascii_case("fb2") {
            let epub_href = format!("/opds/download/{}.epub", b.id);
            self.out.push_str("    <link href=\"");
            self.out.push_str(&escape(&epub_href));
            self.out
                .push_str("\" rel=\"http://opds-spec.org/acquisition\" type=\"application/epub+zip\"/>\n");
        }

        // Self link for the entry feed (some clients want it).
        self.out.push_str("    <link href=\"");
        self.out.push_str(&escape(&book_href));
        self.out.push_str("\" rel=\"alternate\" type=\"application/atom+xml;type=entry;profile=opds-catalog\"/>\n");

        self.out.push_str("  </entry>\n");
    }
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Crude RFC3339 timestamp in UTC. We don't pull in `chrono` just for this.
fn now_iso8601() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d, hh, mm, ss) = epoch_to_civil(secs as i64);
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

/// Days/time-of-day from a unix epoch seconds value. Based on Howard Hinnant's
/// public-domain civil_from_days algorithm — works for the entire Gregorian
/// calendar.
fn epoch_to_civil(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400) as u32;
    let hh = secs_of_day / 3600;
    let mm = (secs_of_day % 3600) / 60;
    let ss = secs_of_day % 60;

    // shift so that 0 == 0000-03-01
    let z = days + 719468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y_civil = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (mp + if mp < 10 { 3 } else { -9 }) as u32;
    let y = if m <= 2 { y_civil + 1 } else { y_civil };
    (y, m, d, hh, mm, ss)
}
