//! Full-text reader: turns a book file (FB2 or EPUB) into a `ReaderBook`
//! consumed by the frontend reader overlay.
//!
//! The output is a list of chapters with sanitized HTML and a TOC tree.
//! Inline images are inlined as `data:` URLs so the webview can render
//! everything without an extra asset-serving channel.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use base64::Engine;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;

use crate::error::{Error, Result};
use crate::model::{ReaderBook, ReaderChapter, TocEntry};

/// Cap individual embedded images so a single illustration doesn't blow the
/// page memory. 6 MB is enough for printable photos at sensible DPI.
const MAX_IMAGE_BYTES: usize = 6 * 1024 * 1024;

pub fn read_reader_book(zip_path: &Path, file: &str, ext: &str) -> Result<ReaderBook> {
    if !zip_path.exists() {
        return Err(Error::NotFound(format!(
            "архив не найден: {}",
            zip_path.display()
        )));
    }
    let lower = ext.to_ascii_lowercase();
    match lower.as_str() {
        "fb2" => {
            let bytes = read_from_zip(zip_path, file, &[("fb2", true)])?;
            parse_fb2(&bytes)
        }
        "epub" => {
            let bytes = read_from_zip(zip_path, file, &[("epub", true)])?;
            parse_epub(&bytes)
        }
        other => Err(Error::Other(format!(
            "формат {} пока не поддерживается ридером",
            other.to_uppercase()
        ))),
    }
}

fn read_from_zip(zip_path: &Path, file: &str, exts: &[(&str, bool)]) -> Result<Vec<u8>> {
    let zf = std::fs::File::open(zip_path)?;
    let mut zip = zip::ZipArchive::new(zf)?;
    let mut candidates: Vec<String> = Vec::new();
    for (ext, _) in exts {
        candidates.push(format!("{file}.{ext}"));
        candidates.push(format!("{file}.{}", ext.to_ascii_uppercase()));
    }
    let mut buf = Vec::new();
    for name in &candidates {
        if let Ok(mut entry) = zip.by_name(name) {
            entry.read_to_end(&mut buf)?;
            return Ok(buf);
        }
    }
    Err(Error::NotFound(format!(
        "файл книги не найден в {}",
        zip_path.display()
    )))
}

// ============================================================================
// FB2
// ============================================================================

/// Map of FB2 `<binary id=...>` payloads — id → data URL.
/// FB2 lists binaries at the end of the file, but body content can reference
/// them by id; resolved in a second pass.
type Binaries = HashMap<String, String>;

#[derive(Default)]
struct Fb2Doc {
    title: String,
    authors: Vec<String>,
    lang: String,
    cover_id: Option<String>,
    cover_url: Option<String>,
    /// Pre-rendered chapters with `data-bin-id="X"` placeholders for images.
    chapters: Vec<ReaderChapter>,
    toc: Vec<TocEntry>,
    binaries: Binaries,
}

fn parse_fb2(bytes: &[u8]) -> Result<ReaderBook> {
    let mut doc = Fb2Doc::default();

    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    reader.config_mut().expand_empty_elements = false;

    let mut buf = Vec::new();
    // We do a single-pass tokenization, dispatching on the *path* of nesting:
    // FictionBook → description → title-info → ...
    //              ↘ body → section → ...
    //              ↘ binary
    let mut path: Vec<String> = Vec::new();
    let mut state = Fb2State::default();

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| Error::Xml(e.into()))?
        {
            Event::Eof => break,
            Event::Start(e) => {
                let name = local_name(e.name().as_ref());
                path.push(name.clone());
                handle_fb2_start(&path, &e, &mut doc, &mut state);
            }
            Event::Empty(e) => {
                let name = local_name(e.name().as_ref());
                path.push(name.clone());
                handle_fb2_start(&path, &e, &mut doc, &mut state);
                handle_fb2_end(&path, &mut doc, &mut state);
                path.pop();
            }
            Event::Text(t) => {
                let text = t
                    .unescape()
                    .map_err(|e| Error::Xml(e.into()))?
                    .into_owned();
                handle_fb2_text(&path, &text, &mut doc, &mut state);
            }
            Event::CData(c) => {
                let s = std::str::from_utf8(c.as_ref()).unwrap_or_default();
                handle_fb2_text(&path, s, &mut doc, &mut state);
            }
            Event::End(_) => {
                handle_fb2_end(&path, &mut doc, &mut state);
                path.pop();
            }
            _ => {}
        }
        buf.clear();
    }

    // Resolve cover.
    if let Some(id) = doc.cover_id.as_ref() {
        if let Some(url) = doc.binaries.get(id) {
            doc.cover_url = Some(url.clone());
        }
    }

    // Resolve image placeholders in chapter HTML.
    for ch in &mut doc.chapters {
        ch.html = resolve_image_placeholders(&ch.html, &doc.binaries);
    }

    Ok(ReaderBook {
        title: doc.title,
        authors: doc.authors,
        lang: doc.lang,
        cover_data_url: doc.cover_url,
        chapters: doc.chapters,
        toc: doc.toc,
        format: "fb2".to_string(),
        position: None,
    })
}

#[derive(Default)]
struct Fb2State {
    in_title_info: bool,
    in_author: bool,
    current_author: AuthorBuf,
    /// Stack of open `<section>` builders. Index 0 is the top-level chapter
    /// currently being assembled; nested entries get spliced back into the
    /// top entry on close.
    section_stack: Vec<SectionBuilder>,
    /// Monotonic counter for anchor ids across the whole document.
    anchor_counter: u32,
    /// True while we're inside a `<title>` tag of the section under construction.
    title_capture_depth: i32,
    /// Buffer of currently-captured title text.
    title_buf: String,
    /// Currently open `<binary>` payload (base64) being collected.
    binary_id: Option<String>,
    binary_mime: Option<String>,
    binary_buf: String,
    /// When `Some`, we're capturing into this string with HTML output.
    /// Inline content sits inside <p>, <emphasis> etc.
    text_target: Option<TextTarget>,
}

impl Fb2State {
    fn next_anchor(&mut self) -> u32 {
        self.anchor_counter += 1;
        self.anchor_counter
    }
}

#[derive(Default)]
struct AuthorBuf {
    last: String,
    first: String,
    middle: String,
    nickname: String,
    field: String, // current sub-tag we're in
}

/// Section being assembled. Each open `<section>` gets its own builder; on
/// close the html is spliced into its parent (so nested sections render
/// inline as headings inside the top-level chapter). Only depth=1 sections
/// flush as standalone `ReaderChapter`s.
struct SectionBuilder {
    /// Chapter id for the *top-level* chapter this section belongs to. Nested
    /// sections share their ancestor's chapter_id and add an anchor.
    chapter_id: String,
    /// Anchor id for jumping into a nested section. None for top-level.
    anchor_id: Option<String>,
    title: Option<String>,
    depth: usize,
    html: String,
    toc_children: Vec<TocEntry>,
}

enum TextTarget {
    /// Append text to the innermost section's HTML buffer.
    Body,
    /// Append text to title buffer.
    Title,
    /// Append text to currently-open binary buffer.
    Binary,
    /// Append text to current author sub-field.
    AuthorField,
}

fn handle_fb2_start(
    path: &[String],
    e: &BytesStart,
    doc: &mut Fb2Doc,
    state: &mut Fb2State,
) {
    let name = path.last().map(String::as_str).unwrap_or("");

    // description metadata
    if path_starts_with(path, &["FictionBook", "description"]) {
        match name {
            "title-info" => state.in_title_info = true,
            "lang" if state.in_title_info => state.text_target = Some(TextTarget::AuthorField),
            "author" if state.in_title_info => {
                state.in_author = true;
                state.current_author = AuthorBuf::default();
            }
            "first-name" | "middle-name" | "last-name" | "nickname"
                if state.in_author =>
            {
                state.current_author.field = name.to_string();
                state.text_target = Some(TextTarget::AuthorField);
            }
            "book-title" if state.in_title_info => {
                state.text_target = Some(TextTarget::AuthorField);
                state.current_author.field = "book-title".to_string();
            }
            "lang" => {
                state.text_target = Some(TextTarget::AuthorField);
                state.current_author.field = "lang".to_string();
            }
            "image" if path.iter().any(|p| p == "coverpage") => {
                if let Some(href) = attr_value(e, "l:href")
                    .or_else(|| attr_value(e, "xlink:href"))
                    .or_else(|| attr_value(e, "href"))
                {
                    doc.cover_id = Some(href.trim_start_matches('#').to_string());
                }
            }
            _ => {}
        }
        return;
    }

    // Binaries are siblings of body inside FictionBook
    if name == "binary" && path.len() == 2 && path[0] == "FictionBook" {
        state.binary_id = attr_value(e, "id");
        state.binary_mime =
            attr_value(e, "content-type").or_else(|| Some("image/jpeg".to_string()));
        state.binary_buf.clear();
        state.text_target = Some(TextTarget::Binary);
        return;
    }

    // Body sections
    if path_starts_with(path, &["FictionBook", "body"]) {
        if name == "section" {
            let depth = state.section_stack.len() + 1;
            let (chapter_id, anchor_id) = if depth == 1 {
                (format!("ch{}", doc.chapters.len()), None)
            } else {
                let parent_id = state
                    .section_stack
                    .first()
                    .map(|s| s.chapter_id.clone())
                    .unwrap_or_else(|| format!("ch{}", doc.chapters.len()));
                let anchor = format!("{parent_id}-s{}", state.next_anchor());
                (parent_id, Some(anchor))
            };
            state.section_stack.push(SectionBuilder {
                chapter_id,
                anchor_id,
                title: None,
                depth,
                html: String::new(),
                toc_children: Vec::new(),
            });
            return;
        }
        if name == "title" && !state.section_stack.is_empty() {
            state.title_capture_depth = 1;
            state.title_buf.clear();
            state.text_target = Some(TextTarget::Title);
            return;
        }

        if state.title_capture_depth > 0 {
            // Inside a <title>: bump depth so children's End events don't close prematurely.
            state.title_capture_depth += 1;
            // Don't emit nested-title tags; we just collect text content.
            return;
        }

        if !state.section_stack.is_empty() {
            // We're inside a section's body — emit HTML for known tags.
            emit_fb2_inline_start(name, e, state);
        }
    }
}

fn handle_fb2_end(path: &[String], doc: &mut Fb2Doc, state: &mut Fb2State) {
    let name = path.last().map(String::as_str).unwrap_or("");

    if path_starts_with(path, &["FictionBook", "description"]) {
        match name {
            "title-info" => state.in_title_info = false,
            "author" if state.in_author => {
                state.in_author = false;
                let display = format_author(&state.current_author);
                if !display.is_empty() {
                    doc.authors.push(display);
                }
            }
            "first-name" | "middle-name" | "last-name" | "nickname"
                if state.in_author =>
            {
                state.text_target = None;
                state.current_author.field.clear();
            }
            "book-title" if state.in_title_info => {
                if doc.title.is_empty() && !state.current_author.field.is_empty() {
                    // book-title text was collected into current_author.field via AuthorField
                    // hack — actually let me re-route: stash separately below.
                }
                state.text_target = None;
            }
            "lang" => {
                state.text_target = None;
            }
            _ => {}
        }
        return;
    }

    if name == "binary" && path.len() == 2 && path[0] == "FictionBook" {
        if let Some(id) = state.binary_id.take() {
            let mime = state.binary_mime.take().unwrap_or_else(|| "image/jpeg".into());
            let cleaned: String = state
                .binary_buf
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect();
            // Validate by decoding then re-encoding (skip if oversized).
            if let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(cleaned.as_bytes())
            {
                if bytes.len() <= MAX_IMAGE_BYTES {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                    doc.binaries.insert(id, format!("data:{mime};base64,{b64}"));
                }
            }
            state.binary_buf.clear();
            state.text_target = None;
        }
        return;
    }

    if path_starts_with(path, &["FictionBook", "body"]) {
        if name == "section" {
            if let Some(sec) = state.section_stack.pop() {
                let fallback_title = format!("Глава {}", doc.chapters.len() + 1);
                let title_str = sec
                    .title
                    .clone()
                    .unwrap_or_else(|| fallback_title.clone());
                let toc_entry = TocEntry {
                    title: title_str.clone(),
                    chapter_id: sec.chapter_id.clone(),
                    anchor: sec.anchor_id.clone(),
                    children: sec.toc_children.clone(),
                };

                if let Some(parent) = state.section_stack.last_mut() {
                    // Nested section → splice into parent's html as a heading
                    // + anchor wrapper, then keep building parent. Depth 2 → h3,
                    // depth 3 → h4, etc. (h2 is reserved for the chapter title).
                    let level = (sec.depth + 1).min(6);
                    if let Some(anchor) = &sec.anchor_id {
                        parent
                            .html
                            .push_str(&format!("<div id=\"{}\">", escape_attr(anchor)));
                    }
                    parent.html.push_str(&format!(
                        "<h{lvl}>{}</h{lvl}>",
                        escape_html(&title_str),
                        lvl = level
                    ));
                    parent.html.push_str(&sec.html);
                    if sec.anchor_id.is_some() {
                        parent.html.push_str("</div>");
                    }
                    parent.toc_children.push(toc_entry);
                } else {
                    // Top-level section → emit a standalone chapter.
                    let mut html = String::new();
                    html.push_str(&format!("<h2>{}</h2>", escape_html(&title_str)));
                    html.push_str(&sec.html);
                    let chapter = ReaderChapter {
                        id: sec.chapter_id,
                        title: sec.title.or_else(|| Some(fallback_title)),
                        html,
                    };
                    doc.chapters.push(chapter);
                    doc.toc.push(toc_entry);
                }
            }
            return;
        }
        if name == "title" && state.title_capture_depth > 0 {
            state.title_capture_depth -= 1;
            if state.title_capture_depth <= 0 {
                state.title_capture_depth = 0;
                state.text_target = None;
                let title = collapse_whitespace(&state.title_buf);
                state.title_buf.clear();
                if let Some(sec) = state.section_stack.last_mut() {
                    if sec.title.is_none() && !title.is_empty() {
                        sec.title = Some(title);
                    }
                }
            } else {
                // closing a nested tag within title — nothing to emit
            }
            return;
        }
        if state.title_capture_depth > 0 {
            state.title_capture_depth -= 1;
            return;
        }
        if !state.section_stack.is_empty() {
            emit_fb2_inline_end(name, state);
        }
    }
}

fn handle_fb2_text(path: &[String], text: &str, doc: &mut Fb2Doc, state: &mut Fb2State) {
    let Some(target) = state.text_target.as_ref() else {
        return;
    };
    match target {
        TextTarget::Title => {
            state.title_buf.push_str(text);
        }
        TextTarget::Binary => {
            state.binary_buf.push_str(text);
        }
        TextTarget::AuthorField => {
            // Dispatch on the most-recent path element for description metadata.
            let parent = path.last().map(String::as_str).unwrap_or("");
            match parent {
                "first-name" => state.current_author.first.push_str(text),
                "middle-name" => state.current_author.middle.push_str(text),
                "last-name" => state.current_author.last.push_str(text),
                "nickname" => state.current_author.nickname.push_str(text),
                "book-title" => doc.title.push_str(text),
                "lang" => doc.lang.push_str(text),
                _ => {}
            }
        }
        TextTarget::Body => {
            if let Some(sec) = state.section_stack.last_mut() {
                sec.html.push_str(&escape_html(text));
            }
        }
    }
}

/// Emit an opening HTML tag for an FB2 inline element within a section body.
fn emit_fb2_inline_start(name: &str, e: &BytesStart, state: &mut Fb2State) {
    let Some(sec) = state.section_stack.last_mut() else {
        return;
    };
    match name {
        "p" => {
            sec.html.push_str("<p>");
            state.text_target = Some(TextTarget::Body);
        }
        "subtitle" => {
            sec.html.push_str("<h4>");
            state.text_target = Some(TextTarget::Body);
        }
        "emphasis" => {
            sec.html.push_str("<em>");
            state.text_target = Some(TextTarget::Body);
        }
        "strong" => {
            sec.html.push_str("<strong>");
            state.text_target = Some(TextTarget::Body);
        }
        "strikethrough" => {
            sec.html.push_str("<s>");
            state.text_target = Some(TextTarget::Body);
        }
        "code" => {
            sec.html.push_str("<code>");
            state.text_target = Some(TextTarget::Body);
        }
        "sub" => {
            sec.html.push_str("<sub>");
            state.text_target = Some(TextTarget::Body);
        }
        "sup" => {
            sec.html.push_str("<sup>");
            state.text_target = Some(TextTarget::Body);
        }
        "cite" => {
            sec.html.push_str("<blockquote class=\"cite\">");
            state.text_target = Some(TextTarget::Body);
        }
        "epigraph" => {
            sec.html.push_str("<blockquote class=\"epigraph\">");
            state.text_target = Some(TextTarget::Body);
        }
        "text-author" => {
            sec.html.push_str("<p class=\"text-author\">");
            state.text_target = Some(TextTarget::Body);
        }
        "poem" => {
            sec.html.push_str("<div class=\"poem\">");
            state.text_target = Some(TextTarget::Body);
        }
        "stanza" => {
            sec.html.push_str("<div class=\"stanza\">");
            state.text_target = Some(TextTarget::Body);
        }
        "v" => {
            sec.html.push_str("<div class=\"verse\">");
            state.text_target = Some(TextTarget::Body);
        }
        "empty-line" => {
            sec.html.push_str("<div class=\"empty-line\"></div>");
        }
        "image" => {
            if let Some(href) = attr_value(e, "l:href")
                .or_else(|| attr_value(e, "xlink:href"))
                .or_else(|| attr_value(e, "href"))
            {
                let id = href.trim_start_matches('#');
                sec.html
                    .push_str(&format!("<img data-bin-id=\"{}\" />", escape_attr(id)));
            }
        }
        "a" => {
            // Strip hrefs to avoid loading external resources; surface as italic span.
            sec.html.push_str("<span class=\"a\">");
            state.text_target = Some(TextTarget::Body);
        }
        "table" => {
            sec.html.push_str("<table>");
            state.text_target = Some(TextTarget::Body);
        }
        "tr" => {
            sec.html.push_str("<tr>");
            state.text_target = Some(TextTarget::Body);
        }
        "td" => {
            sec.html.push_str("<td>");
            state.text_target = Some(TextTarget::Body);
        }
        "th" => {
            sec.html.push_str("<th>");
            state.text_target = Some(TextTarget::Body);
        }
        _ => {
            // Unknown tag — keep capturing text but don't emit element.
            state.text_target = Some(TextTarget::Body);
        }
    }
}

fn emit_fb2_inline_end(name: &str, state: &mut Fb2State) {
    let Some(sec) = state.section_stack.last_mut() else {
        return;
    };
    match name {
        "p" => sec.html.push_str("</p>"),
        "subtitle" => sec.html.push_str("</h4>"),
        "emphasis" => sec.html.push_str("</em>"),
        "strong" => sec.html.push_str("</strong>"),
        "strikethrough" => sec.html.push_str("</s>"),
        "code" => sec.html.push_str("</code>"),
        "sub" => sec.html.push_str("</sub>"),
        "sup" => sec.html.push_str("</sup>"),
        "cite" | "epigraph" => sec.html.push_str("</blockquote>"),
        "text-author" => sec.html.push_str("</p>"),
        "poem" | "stanza" | "v" => sec.html.push_str("</div>"),
        "a" => sec.html.push_str("</span>"),
        "table" => sec.html.push_str("</table>"),
        "tr" => sec.html.push_str("</tr>"),
        "td" => sec.html.push_str("</td>"),
        "th" => sec.html.push_str("</th>"),
        _ => {}
    }
}

fn resolve_image_placeholders(html: &str, binaries: &Binaries) -> String {
    // Replace `<img data-bin-id="X" />` with real src=… data URL, or drop
    // if not found. Doing it with simple string scanning since the tag shape
    // is fixed.
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(idx) = rest.find("<img data-bin-id=\"") {
        out.push_str(&rest[..idx]);
        let after = &rest[idx + "<img data-bin-id=\"".len()..];
        if let Some(end_q) = after.find('"') {
            let id = &after[..end_q];
            let tail = &after[end_q..];
            let after_tag = tail.find("/>").map(|p| &tail[p + 2..]).unwrap_or(tail);
            if let Some(url) = binaries.get(id) {
                out.push_str(&format!(
                    "<img src=\"{}\" alt=\"\" loading=\"lazy\" />",
                    escape_attr(url)
                ));
            }
            rest = after_tag;
        } else {
            out.push_str(rest);
            return out;
        }
    }
    out.push_str(rest);
    out
}

fn format_author(a: &AuthorBuf) -> String {
    let parts: Vec<&str> = [&a.last, &a.first, &a.middle]
        .into_iter()
        .map(String::as_str)
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        a.nickname.clone()
    } else {
        parts.join(" ")
    }
}

// ============================================================================
// EPUB
// ============================================================================

fn parse_epub(bytes: &[u8]) -> Result<ReaderBook> {
    let cursor = std::io::Cursor::new(bytes);
    let mut zip = zip::ZipArchive::new(cursor)?;

    // 1. container.xml → OPF path
    let opf_path = find_opf_path(&mut zip)?;
    let opf_bytes = read_zip_file(&mut zip, &opf_path)?;
    let opf = parse_opf(&opf_bytes)?;

    let opf_dir = path_dir(&opf_path);

    // 2. Read each spine chapter and a few media assets we need.
    let mut chapters: Vec<ReaderChapter> = Vec::new();
    let mut href_to_chapter: HashMap<String, String> = HashMap::new();

    for (idx, spine_idref) in opf.spine.iter().enumerate() {
        let Some(item) = opf.manifest.get(spine_idref) else {
            continue;
        };
        if !item.media_type.contains("xhtml") && !item.media_type.contains("html") {
            continue;
        }
        let href = join_path(&opf_dir, &item.href);
        let raw = match read_zip_file(&mut zip, &href) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let body_html = extract_xhtml_body(&raw)?;
        let asset_dir = path_dir(&href);
        let chapter_html =
            rewrite_xhtml_assets(&body_html, &mut zip, &asset_dir, &mut zip_resource_cache());
        let chapter_id = format!("ch{idx}");

        // Title: try first heading in the body, fall back to manifest id.
        let title = first_heading_text(&chapter_html)
            .or_else(|| opf.toc_titles.get(&href).cloned())
            .or(None);

        href_to_chapter.insert(href.clone(), chapter_id.clone());
        // Also register without anchor variations
        chapters.push(ReaderChapter {
            id: chapter_id,
            title,
            html: chapter_html,
        });
    }

    // 3. TOC via nav.xhtml (EPUB3) or NCX (EPUB2).
    let toc = if let Some(nav_href) = opf.nav_href.as_ref() {
        let nav_path = join_path(&opf_dir, nav_href);
        let nav_dir = path_dir(&nav_path);
        if let Ok(nav_bytes) = read_zip_file(&mut zip, &nav_path) {
            parse_nav(&nav_bytes, &nav_dir, &href_to_chapter).unwrap_or_default()
        } else {
            vec![]
        }
    } else if let Some(ncx_href) = opf.ncx_href.as_ref() {
        let ncx_path = join_path(&opf_dir, ncx_href);
        let ncx_dir = path_dir(&ncx_path);
        if let Ok(ncx_bytes) = read_zip_file(&mut zip, &ncx_path) {
            parse_ncx(&ncx_bytes, &ncx_dir, &href_to_chapter).unwrap_or_default()
        } else {
            vec![]
        }
    } else {
        vec![]
    };

    let toc = if toc.is_empty() {
        // Fallback: one TOC entry per chapter.
        chapters
            .iter()
            .enumerate()
            .map(|(i, c)| TocEntry {
                title: c
                    .title
                    .clone()
                    .unwrap_or_else(|| format!("Глава {}", i + 1)),
                chapter_id: c.id.clone(),
                anchor: None,
                children: vec![],
            })
            .collect()
    } else {
        toc
    };

    // 4. Cover image.
    let cover_data_url = if let Some(href) = opf.cover_href.as_ref() {
        let cover_path = join_path(&opf_dir, href);
        read_zip_file(&mut zip, &cover_path)
            .ok()
            .filter(|b| b.len() <= MAX_IMAGE_BYTES)
            .map(|bytes| {
                let mime = guess_image_mime(&cover_path);
                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                format!("data:{mime};base64,{b64}")
            })
    } else {
        None
    };

    Ok(ReaderBook {
        title: opf.title,
        authors: opf.authors,
        lang: opf.lang,
        cover_data_url,
        chapters,
        toc,
        format: "epub".to_string(),
        position: None,
    })
}

struct Opf {
    title: String,
    authors: Vec<String>,
    lang: String,
    /// item id → manifest item
    manifest: HashMap<String, ManifestItem>,
    /// ordered spine idrefs
    spine: Vec<String>,
    /// href to nav.xhtml (relative to OPF), if EPUB3
    nav_href: Option<String>,
    /// href to NCX, if EPUB2
    ncx_href: Option<String>,
    cover_href: Option<String>,
    /// Manifest-discovered titles by full zip path → title
    toc_titles: HashMap<String, String>,
}

struct ManifestItem {
    href: String,
    media_type: String,
}

fn parse_opf(bytes: &[u8]) -> Result<Opf> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    reader.config_mut().expand_empty_elements = false;

    let mut opf = Opf {
        title: String::new(),
        authors: Vec::new(),
        lang: String::new(),
        manifest: HashMap::new(),
        spine: Vec::new(),
        nav_href: None,
        ncx_href: None,
        cover_href: None,
        toc_titles: HashMap::new(),
    };
    let mut ncx_id: Option<String> = None;
    let mut cover_id_meta: Option<String> = None;
    let mut buf = Vec::new();
    let mut path: Vec<String> = Vec::new();
    let mut text_target: Option<String> = None; // local name being captured

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| Error::Xml(e.into()))?
        {
            Event::Eof => break,
            Event::Start(e) => {
                let name = local_name(e.name().as_ref());
                path.push(name.clone());
                if name == "title" || name == "creator" || name == "language" {
                    text_target = Some(name);
                } else if name == "item" {
                    let id = attr_value(&e, "id").unwrap_or_default();
                    let href = attr_value(&e, "href").unwrap_or_default();
                    let media_type = attr_value(&e, "media-type").unwrap_or_default();
                    let properties = attr_value(&e, "properties").unwrap_or_default();
                    if properties.contains("nav") {
                        opf.nav_href = Some(href.clone());
                    }
                    if properties.contains("cover-image") {
                        opf.cover_href = Some(href.clone());
                    }
                    opf.manifest.insert(
                        id,
                        ManifestItem { href, media_type },
                    );
                } else if name == "itemref" {
                    if let Some(idref) = attr_value(&e, "idref") {
                        opf.spine.push(idref);
                    }
                } else if name == "spine" {
                    ncx_id = attr_value(&e, "toc");
                } else if name == "meta" {
                    // EPUB2 cover: <meta name="cover" content="cover-item-id"/>
                    let n = attr_value(&e, "name").unwrap_or_default();
                    if n == "cover" {
                        cover_id_meta = attr_value(&e, "content");
                    }
                }
            }
            Event::Empty(e) => {
                let name = local_name(e.name().as_ref());
                path.push(name.clone());
                if name == "item" {
                    let id = attr_value(&e, "id").unwrap_or_default();
                    let href = attr_value(&e, "href").unwrap_or_default();
                    let media_type = attr_value(&e, "media-type").unwrap_or_default();
                    let properties = attr_value(&e, "properties").unwrap_or_default();
                    if properties.contains("nav") {
                        opf.nav_href = Some(href.clone());
                    }
                    if properties.contains("cover-image") {
                        opf.cover_href = Some(href.clone());
                    }
                    opf.manifest.insert(
                        id,
                        ManifestItem { href, media_type },
                    );
                } else if name == "itemref" {
                    if let Some(idref) = attr_value(&e, "idref") {
                        opf.spine.push(idref);
                    }
                } else if name == "meta" {
                    let n = attr_value(&e, "name").unwrap_or_default();
                    if n == "cover" {
                        cover_id_meta = attr_value(&e, "content");
                    }
                }
                path.pop();
            }
            Event::Text(t) => {
                if let Some(name) = text_target.as_ref() {
                    let text = t
                        .unescape()
                        .map_err(|e| Error::Xml(e.into()))?
                        .into_owned();
                    match name.as_str() {
                        "title" if opf.title.is_empty() => opf.title.push_str(&text),
                        "creator" => {
                            let t = text.trim();
                            if !t.is_empty() {
                                opf.authors.push(t.to_string());
                            }
                        }
                        "language" if opf.lang.is_empty() => opf.lang.push_str(text.trim()),
                        _ => {}
                    }
                }
            }
            Event::End(_) => {
                if !path.is_empty() {
                    path.pop();
                }
                text_target = None;
            }
            _ => {}
        }
        buf.clear();
    }

    // Resolve NCX href from manifest id.
    if let Some(id) = ncx_id.as_ref() {
        if let Some(it) = opf.manifest.get(id) {
            opf.ncx_href = Some(it.href.clone());
        }
    }
    // Fallback: any item with media-type x-dtbncx+xml.
    if opf.ncx_href.is_none() {
        for it in opf.manifest.values() {
            if it.media_type == "application/x-dtbncx+xml" {
                opf.ncx_href = Some(it.href.clone());
                break;
            }
        }
    }
    // EPUB2 cover via meta name="cover"
    if opf.cover_href.is_none() {
        if let Some(id) = cover_id_meta.as_ref() {
            if let Some(it) = opf.manifest.get(id) {
                opf.cover_href = Some(it.href.clone());
            }
        }
    }

    Ok(opf)
}

fn find_opf_path<R: Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>) -> Result<String> {
    let bytes = read_zip_file(zip, "META-INF/container.xml")?;
    let mut reader = Reader::from_reader(bytes.as_slice());
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| Error::Xml(e.into()))?
        {
            Event::Eof => break,
            Event::Empty(e) | Event::Start(e) => {
                if local_name(e.name().as_ref()) == "rootfile" {
                    if let Some(path) = attr_value(&e, "full-path") {
                        return Ok(path);
                    }
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Err(Error::Other("OPF не найден в EPUB".into()))
}

fn read_zip_file<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>> {
    let mut entry = zip.by_name(name).map_err(|_| {
        Error::NotFound(format!("файл {name} не найден в архиве EPUB"))
    })?;
    let mut buf = Vec::new();
    entry.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Extract everything between `<body ...>` and `</body>` from an XHTML chapter.
fn extract_xhtml_body(bytes: &[u8]) -> Result<String> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    reader.config_mut().expand_empty_elements = false;

    let mut out = String::new();
    let mut in_body = false;
    let mut depth = 0i32;
    let mut buf = Vec::new();
    // We sanitize as we go: drop <script>, <style>, on* attributes.
    let mut skip_depth = 0i32;
    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| Error::Xml(e.into()))?
        {
            Event::Eof => break,
            Event::Start(e) => {
                let name = local_name(e.name().as_ref());
                if !in_body {
                    if name == "body" {
                        in_body = true;
                        depth = 0;
                    }
                    continue;
                }
                if skip_depth > 0 {
                    skip_depth += 1;
                    continue;
                }
                if name == "script" || name == "style" || name == "head" {
                    skip_depth = 1;
                    continue;
                }
                depth += 1;
                emit_xhtml_start(&name, &e, &mut out);
            }
            Event::Empty(e) => {
                if !in_body {
                    continue;
                }
                if skip_depth > 0 {
                    continue;
                }
                let name = local_name(e.name().as_ref());
                if name == "script" || name == "style" {
                    continue;
                }
                emit_xhtml_start(&name, &e, &mut out);
                emit_xhtml_end(&name, &mut out, true);
            }
            Event::Text(t) => {
                if !in_body || skip_depth > 0 {
                    continue;
                }
                let text = t
                    .unescape()
                    .map_err(|e| Error::Xml(e.into()))?
                    .into_owned();
                out.push_str(&escape_html(&text));
            }
            Event::CData(c) => {
                if !in_body || skip_depth > 0 {
                    continue;
                }
                let s = std::str::from_utf8(c.as_ref()).unwrap_or_default();
                out.push_str(&escape_html(s));
            }
            Event::End(e) => {
                let name = local_name(e.name().as_ref());
                if !in_body {
                    continue;
                }
                if skip_depth > 0 {
                    skip_depth -= 1;
                    continue;
                }
                if name == "body" {
                    in_body = false;
                    continue;
                }
                depth -= 1;
                emit_xhtml_end(&name, &mut out, false);
            }
            _ => {}
        }
        buf.clear();
        let _ = depth;
    }
    Ok(out)
}

/// Allowlist of tags we'll let through from EPUB XHTML.
const SAFE_XHTML_TAGS: &[&str] = &[
    "p", "br", "hr", "div", "span", "section", "article", "header", "footer", "nav",
    "h1", "h2", "h3", "h4", "h5", "h6",
    "em", "i", "strong", "b", "u", "s", "small", "sub", "sup", "code", "pre", "kbd",
    "blockquote", "cite",
    "ul", "ol", "li", "dl", "dt", "dd",
    "table", "thead", "tbody", "tfoot", "tr", "td", "th",
    "a", "img", "figure", "figcaption",
];

fn emit_xhtml_start(name: &str, e: &BytesStart, out: &mut String) {
    if !SAFE_XHTML_TAGS.contains(&name) {
        // Unknown tag: emit nothing, keep its text content.
        return;
    }
    out.push('<');
    out.push_str(name);
    for attr in e.attributes().flatten() {
        let k = std::str::from_utf8(attr.key.as_ref()).unwrap_or("");
        let key_low = k.to_ascii_lowercase();
        let local = if let Some(pos) = key_low.find(':') {
            &key_low[pos + 1..]
        } else {
            key_low.as_str()
        };
        // Block scripty attributes
        if local.starts_with("on") || local == "style" || local == "class" || local == "id" {
            // Allow class and id for navigation/structure.
            if local != "class" && local != "id" {
                continue;
            }
        }
        // Allow only common safe attributes.
        let allow = matches!(
            local,
            "href"
                | "src"
                | "alt"
                | "title"
                | "id"
                | "class"
                | "colspan"
                | "rowspan"
                | "width"
                | "height"
        );
        if !allow {
            continue;
        }
        let raw = std::str::from_utf8(attr.value.as_ref()).unwrap_or("");
        // For href: keep only fragment links; strip external (we don't navigate).
        let value = if local == "href" {
            if raw.starts_with('#') {
                raw.to_string()
            } else {
                continue;
            }
        } else if local == "src" {
            // Will be rewritten in rewrite_xhtml_assets — keep marker.
            raw.to_string()
        } else {
            raw.to_string()
        };
        out.push(' ');
        out.push_str(local);
        out.push_str("=\"");
        out.push_str(&escape_attr(&value));
        out.push('"');
    }
    out.push('>');
}

fn emit_xhtml_end(name: &str, out: &mut String, self_close: bool) {
    if !SAFE_XHTML_TAGS.contains(&name) {
        return;
    }
    if self_close {
        // Already wrote opening tag; close it.
        if matches!(name, "br" | "hr" | "img") {
            return; // void elements — no closing tag needed
        }
    }
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

/// In-place rewrite of `src="..."` URLs in XHTML body → data URLs.
fn rewrite_xhtml_assets<R: Read + std::io::Seek>(
    html: &str,
    zip: &mut zip::ZipArchive<R>,
    chapter_dir: &str,
    cache: &mut HashMap<String, Option<String>>,
) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    loop {
        let Some(idx) = rest.find("src=\"") else {
            out.push_str(rest);
            break;
        };
        out.push_str(&rest[..idx]);
        let after = &rest[idx + "src=\"".len()..];
        let Some(end_q) = after.find('"') else {
            out.push_str(&rest[idx..]);
            break;
        };
        let raw_src = &after[..end_q];
        let tail = &after[end_q + 1..];
        let resolved = if raw_src.starts_with("data:") {
            Some(raw_src.to_string())
        } else {
            let asset_path = join_path(chapter_dir, raw_src);
            if let Some(cached) = cache.get(&asset_path) {
                cached.clone()
            } else {
                let url = read_zip_file(zip, &asset_path)
                    .ok()
                    .filter(|b| b.len() <= MAX_IMAGE_BYTES)
                    .map(|bytes| {
                        let mime = guess_image_mime(&asset_path);
                        let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                        format!("data:{mime};base64,{b64}")
                    });
                cache.insert(asset_path.clone(), url.clone());
                url
            }
        };
        if let Some(url) = resolved {
            out.push_str("src=\"");
            out.push_str(&escape_attr(&url));
            out.push('"');
        } else {
            out.push_str("src=\"\"");
        }
        rest = tail;
    }
    out
}

fn zip_resource_cache() -> HashMap<String, Option<String>> {
    HashMap::new()
}

fn parse_nav(
    bytes: &[u8],
    nav_dir: &str,
    href_to_chapter: &HashMap<String, String>,
) -> Result<Vec<TocEntry>> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    reader.config_mut().expand_empty_elements = false;

    let mut stack: Vec<NavBuilder> = Vec::new();
    let mut roots: Vec<TocEntry> = Vec::new();
    let mut in_toc_nav = false;
    // Capture text inside <li> *before* its nested <ol>/<ul> opens.
    let mut list_depth_inside_li: Vec<i32> = Vec::new();
    let mut buf = Vec::new();

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| Error::Xml(e.into()))?
        {
            Event::Eof => break,
            Event::Start(e) => {
                let name = local_name(e.name().as_ref());
                if name == "nav" && !in_toc_nav {
                    let t = attr_value(&e, "epub:type")
                        .or_else(|| attr_value(&e, "type"))
                        .or_else(|| attr_value(&e, "role"))
                        .unwrap_or_default();
                    if t.is_empty() || t.to_ascii_lowercase().contains("toc") {
                        in_toc_nav = true;
                    }
                    continue;
                }
                if !in_toc_nav {
                    continue;
                }
                match name.as_str() {
                    "ol" | "ul" => {
                        if let Some(top) = list_depth_inside_li.last_mut() {
                            *top += 1;
                        }
                    }
                    "li" => {
                        stack.push(NavBuilder::default());
                        list_depth_inside_li.push(0);
                    }
                    "a" => {
                        if list_depth_inside_li.last().copied().unwrap_or(1) == 0 {
                            if let Some(top) = stack.last_mut() {
                                if top.href.is_none() {
                                    top.href = attr_value(&e, "href");
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Empty(e) => {
                if !in_toc_nav {
                    continue;
                }
                let name = local_name(e.name().as_ref());
                if name == "a" && list_depth_inside_li.last().copied().unwrap_or(1) == 0 {
                    if let Some(top) = stack.last_mut() {
                        if top.href.is_none() {
                            top.href = attr_value(&e, "href");
                        }
                    }
                }
            }
            Event::Text(t) => {
                if !in_toc_nav {
                    continue;
                }
                // Append text only when current li hasn't entered its nested list yet.
                if list_depth_inside_li.last().copied().unwrap_or(1) == 0 {
                    if let Some(top) = stack.last_mut() {
                        let txt = t
                            .unescape()
                            .map_err(|e| Error::Xml(e.into()))?
                            .into_owned();
                        top.title_buf.push_str(&txt);
                    }
                }
            }
            Event::End(e) => {
                let name = local_name(e.name().as_ref());
                if name == "nav" && in_toc_nav {
                    in_toc_nav = false;
                    continue;
                }
                if !in_toc_nav {
                    continue;
                }
                match name.as_str() {
                    "ol" | "ul" => {
                        if let Some(top) = list_depth_inside_li.last_mut() {
                            if *top > 0 {
                                *top -= 1;
                            }
                        }
                    }
                    "li" => {
                        list_depth_inside_li.pop();
                        if let Some(top) = stack.pop() {
                            let entry = top.build(nav_dir, href_to_chapter);
                            if let Some(parent) = stack.last_mut() {
                                parent.children.push(entry);
                            } else {
                                roots.push(entry);
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(roots)
}

fn parse_ncx(
    bytes: &[u8],
    ncx_dir: &str,
    href_to_chapter: &HashMap<String, String>,
) -> Result<Vec<TocEntry>> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    reader.config_mut().expand_empty_elements = false;

    let mut stack: Vec<NavBuilder> = Vec::new();
    let mut roots: Vec<TocEntry> = Vec::new();
    let mut buf = Vec::new();
    let mut in_text = false;

    loop {
        match reader
            .read_event_into(&mut buf)
            .map_err(|e| Error::Xml(e.into()))?
        {
            Event::Eof => break,
            Event::Start(e) => {
                let name = local_name(e.name().as_ref());
                match name.as_str() {
                    "navPoint" => stack.push(NavBuilder::default()),
                    "text" => in_text = true,
                    _ => {}
                }
            }
            Event::Empty(e) => {
                let name = local_name(e.name().as_ref());
                if name == "content" {
                    if let Some(top) = stack.last_mut() {
                        if top.href.is_none() {
                            top.href = attr_value(&e, "src");
                        }
                    }
                }
            }
            Event::Text(t) => {
                if in_text {
                    if let Some(top) = stack.last_mut() {
                        let txt = t
                            .unescape()
                            .map_err(|e| Error::Xml(e.into()))?
                            .into_owned();
                        top.title_buf.push_str(&txt);
                    }
                }
            }
            Event::End(e) => {
                let name = local_name(e.name().as_ref());
                match name.as_str() {
                    "text" => in_text = false,
                    "navPoint" => {
                        if let Some(top) = stack.pop() {
                            let entry = top.build(ncx_dir, href_to_chapter);
                            if let Some(parent) = stack.last_mut() {
                                parent.children.push(entry);
                            } else {
                                roots.push(entry);
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(roots)
}

#[derive(Default)]
struct NavBuilder {
    title_buf: String,
    href: Option<String>,
    children: Vec<TocEntry>,
}

impl NavBuilder {
    fn build(self, base_dir: &str, href_to_chapter: &HashMap<String, String>) -> TocEntry {
        let mut chapter_id = String::new();
        let mut anchor: Option<String> = None;
        if let Some(href) = self.href {
            let (file, anchor_part) = match href.find('#') {
                Some(i) => (href[..i].to_string(), Some(href[i + 1..].to_string())),
                None => (href, None),
            };
            anchor = anchor_part;
            let path = join_path(base_dir, &file);
            if let Some(id) = href_to_chapter.get(&path) {
                chapter_id = id.clone();
            }
        }
        let title = collapse_whitespace(&self.title_buf);
        TocEntry {
            title: if title.is_empty() {
                "(без названия)".to_string()
            } else {
                title
            },
            chapter_id,
            anchor,
            children: self.children,
        }
    }
}

fn first_heading_text(html: &str) -> Option<String> {
    // Find first <hN>...</hN> with N in 1..=4.
    for tag in ["h1", "h2", "h3", "h4"] {
        let open = format!("<{tag}");
        if let Some(start) = html.find(&open) {
            if let Some(gt) = html[start..].find('>') {
                let after = start + gt + 1;
                let close = format!("</{tag}>");
                if let Some(end) = html[after..].find(&close) {
                    let inner = &html[after..after + end];
                    let clean = strip_tags(inner);
                    let s = collapse_whitespace(&clean);
                    if !s.is_empty() {
                        return Some(s);
                    }
                }
            }
        }
    }
    None
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

// ============================================================================
// Path helpers (work on ZIP-internal /-separated paths)
// ============================================================================

fn path_dir(p: &str) -> String {
    match p.rfind('/') {
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}

fn join_path(base: &str, rel: &str) -> String {
    // Resolve `./` and `../` segments.
    let combined = if rel.starts_with('/') {
        rel.trim_start_matches('/').to_string()
    } else if base.is_empty() {
        rel.to_string()
    } else {
        format!("{base}/{rel}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for seg in combined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn guess_image_mime(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else {
        "image/jpeg"
    }
}

// ============================================================================
// XML helpers
// ============================================================================

fn attr_value(e: &BytesStart, key: &str) -> Option<String> {
    for a in e.attributes().flatten() {
        let k = std::str::from_utf8(a.key.as_ref()).unwrap_or("");
        if k.eq_ignore_ascii_case(key) {
            return std::str::from_utf8(a.value.as_ref())
                .ok()
                .map(str::to_string);
        }
        // Also try matching by local-name (strip namespace prefix on key).
        if let Some(pos) = k.find(':') {
            if k[pos + 1..].eq_ignore_ascii_case(key) {
                return std::str::from_utf8(a.value.as_ref())
                    .ok()
                    .map(str::to_string);
            }
        }
        if let Some(pos) = key.find(':') {
            if k.eq_ignore_ascii_case(&key[pos + 1..]) {
                return std::str::from_utf8(a.value.as_ref())
                    .ok()
                    .map(str::to_string);
            }
        }
    }
    None
}

fn local_name(name: &[u8]) -> String {
    let s = std::str::from_utf8(name).unwrap_or("");
    if let Some(pos) = s.find(':') {
        s[pos + 1..].to_string()
    } else {
        s.to_string()
    }
}

fn path_starts_with(path: &[String], prefix: &[&str]) -> bool {
    if path.len() < prefix.len() {
        return false;
    }
    for (i, p) in prefix.iter().enumerate() {
        if !path[i].eq_ignore_ascii_case(p) {
            return false;
        }
    }
    true
}

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("&quot;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            _ => out.push(ch),
        }
    }
    out
}

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for ch in s.chars() {
        if ch.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FB2_SAMPLE: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
  <description>
    <title-info>
      <book-title>Test Book</book-title>
      <author><first-name>Иван</first-name><last-name>Петров</last-name></author>
      <lang>ru</lang>
      <coverpage><image l:href="#cover.jpg"/></coverpage>
    </title-info>
  </description>
  <body>
    <section>
      <title><p>Chapter 1</p></title>
      <p>Hello <emphasis>world</emphasis>!</p>
      <image l:href="#pic.jpg"/>
      <section>
        <title><p>1.1 Sub</p></title>
        <p>nested content</p>
      </section>
    </section>
    <section>
      <title><p>Chapter 2</p></title>
      <p>second</p>
    </section>
  </body>
  <binary id="cover.jpg" content-type="image/jpeg">SGVsbG8=</binary>
  <binary id="pic.jpg" content-type="image/png">SGk=</binary>
</FictionBook>"##;

    #[test]
    fn fb2_parses_chapters_and_toc() {
        let book = parse_fb2(FB2_SAMPLE.as_bytes()).unwrap();
        assert_eq!(book.title, "Test Book");
        assert_eq!(book.authors, vec!["Петров Иван"]);
        assert_eq!(book.lang, "ru");
        assert!(book.cover_data_url.is_some());
        // Only top-level sections become standalone chapters; nested sections
        // are inlined as headings inside their parent.
        assert_eq!(book.chapters.len(), 2);
        let first = &book.chapters[0];
        assert_eq!(first.title.as_deref(), Some("Chapter 1"));
        assert!(first.html.starts_with("<h2>Chapter 1</h2>"));
        assert!(first.html.contains("<p>Hello <em>world</em>!</p>"));
        assert!(first.html.contains("data:image/png;base64"));
        // Nested "1.1 Sub" appears inline within the parent chapter as a deeper heading.
        assert!(first.html.contains("<h3>1.1 Sub</h3>"));
        assert!(first.html.contains("id=\"ch0-s1\""));
        assert_eq!(book.toc.len(), 2);
        assert_eq!(book.toc[0].children.len(), 1);
        assert_eq!(book.toc[0].children[0].anchor.as_deref(), Some("ch0-s1"));
        assert_eq!(book.toc[0].children[0].chapter_id, "ch0");
    }

    #[test]
    fn escape_html_works() {
        assert_eq!(escape_html("a < b & c"), "a &lt; b &amp; c");
    }

    #[test]
    fn path_join_normalizes() {
        assert_eq!(join_path("OEBPS", "ch1.xhtml"), "OEBPS/ch1.xhtml");
        assert_eq!(join_path("OEBPS/text", "../images/c.jpg"), "OEBPS/images/c.jpg");
        assert_eq!(join_path("", "ch1.xhtml"), "ch1.xhtml");
    }
}
