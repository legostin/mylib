//! On-the-fly FB2 → EPUB conversion. We stream the FB2 with quick-xml and
//! emit XHTML chunks as we go, packing everything into a valid EPUB 3 zip at
//! the end. Aim is "good enough for ebook readers" — section structure, basic
//! inline formatting, cover, and inline images. Footnote bodies and tables
//! are flattened pragmatically.

use std::collections::HashMap;
use std::io::{Cursor, Write};

use base64::Engine;
use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

use crate::error::{Error, Result};

pub struct ConvertedEpub {
    pub bytes: Vec<u8>,
    /// Suggested file stem (without ".epub" extension), derived from the FB2
    /// title. Used for download `filename=`.
    pub filename_stem: String,
}

pub fn convert_fb2_to_epub(fb2: &[u8], identifier: &str) -> Result<ConvertedEpub> {
    let parsed = parse(fb2)?;
    let stem = sanitize_stem(&parsed.title);
    let bytes = assemble_epub(&parsed, identifier)?;
    Ok(ConvertedEpub {
        bytes,
        filename_stem: stem,
    })
}

// ---------- parsing ---------------------------------------------------------

#[derive(Default)]
struct Parsed {
    title: String,
    authors: Vec<String>,
    lang: String,
    annotation_html: String,
    body_html: String,
    cover_id: Option<String>,
    binaries: HashMap<String, Binary>,
}

struct Binary {
    mime: String,
    data: Vec<u8>,
}

#[derive(Default)]
struct State {
    stack: Vec<String>,

    // Metadata
    title: Option<String>,
    authors: Vec<String>,
    lang: Option<String>,
    cover_id: Option<String>,

    // Whether we've already seen one <body> — extra ones (like footnote
    // sections) get appended below the main content with a divider so
    // <a href="#n1"> still resolves to something in the same xhtml file.
    body_count: u32,

    // Author name parts buffered while inside <author>.
    author_first: String,
    author_middle: String,
    author_last: String,

    // Renderers — output sinks for different parts of the document.
    body_html: String,
    annotation_html: String,

    // Coverpage flag — we want the first <image> inside it as cover.
    in_coverpage: bool,

    // Binary collection.
    current_binary_id: Option<String>,
    current_binary_mime: Option<String>,
    current_binary_b64: String,
    binaries: HashMap<String, Binary>,
}

impl State {
    fn in_tag(&self, name: &str) -> bool {
        self.stack.iter().any(|s| s == name)
    }
    fn in_description(&self) -> bool {
        self.in_tag("description")
    }
    fn in_annotation(&self) -> bool {
        self.in_tag("annotation")
    }
    fn in_title_info(&self) -> bool {
        self.in_tag("title-info")
    }
    fn in_body(&self) -> bool {
        self.in_tag("body") && !self.in_description() && !self.in_tag("binary")
    }
    fn in_binary(&self) -> bool {
        self.in_tag("binary")
    }
    fn in_author(&self) -> bool {
        self.in_tag("author") && self.in_title_info()
    }
    fn parent(&self) -> Option<&str> {
        self.stack.last().map(|s| s.as_str())
    }
}

fn parse(bytes: &[u8]) -> Result<Parsed> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    reader.config_mut().expand_empty_elements = false;

    let mut st = State::default();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => return Err(Error::Xml(e.into())),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => handle_start(&mut st, &e, false),
            Ok(Event::Empty(e)) => {
                handle_start(&mut st, &e, true);
                let name = local_name(e.name().as_ref());
                handle_end(&mut st, &name, true);
            }
            Ok(Event::Text(t)) => {
                let txt = t.unescape().map_err(|e| Error::Xml(e.into()))?;
                handle_text(&mut st, txt.as_ref());
            }
            Ok(Event::CData(c)) => {
                let txt = std::str::from_utf8(c.as_ref()).unwrap_or_default();
                handle_text(&mut st, txt);
            }
            Ok(Event::End(e)) => {
                let name = local_name(e.name().as_ref());
                handle_end(&mut st, &name, false);
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(Parsed {
        title: st.title.unwrap_or_else(|| "Без названия".into()),
        authors: st.authors,
        lang: st.lang.unwrap_or_else(|| "ru".into()),
        annotation_html: st.annotation_html,
        body_html: st.body_html,
        cover_id: st.cover_id,
        binaries: st.binaries,
    })
}

fn handle_start(st: &mut State, e: &BytesStart, self_closing: bool) {
    let name = local_name(e.name().as_ref());

    // Track <body> count BEFORE pushing to stack so the gating logic below
    // sees the new state too.
    if name == "body" && !st.in_description() {
        st.body_count += 1;
        if st.body_count > 1 {
            // Render a divider between bodies — useful for footnote sections.
            st.body_html
                .push_str("\n<hr class=\"body-divider\"/>\n<section class=\"fb2-aux-body\">\n");
        }
    }

    // <binary> needs id/mime captured before children.
    if name == "binary" {
        st.current_binary_id = attr(e, "id");
        st.current_binary_mime = attr(e, "content-type")
            .or_else(|| Some("application/octet-stream".into()));
        st.current_binary_b64.clear();
    }

    // <coverpage> just flips a flag; the <image> inside provides cover id.
    if name == "coverpage" && st.in_title_info() {
        st.in_coverpage = true;
    }

    // Cover capture from <coverpage><image l:href="#cover.jpg"/></coverpage>.
    if name == "image" && st.in_coverpage && st.cover_id.is_none() {
        if let Some(href) = href_attr(e) {
            st.cover_id = Some(href.trim_start_matches('#').to_string());
        }
    }

    // Push to stack BEFORE rendering — so `in_body()` checks include this
    // tag for nested calls; popped on End.
    st.stack.push(name.clone());

    // Render only inside <body>, never inside <description>.
    if st.in_body() {
        render_open(st, &name, e, self_closing);
    } else if st.in_annotation() {
        render_annotation_open(st, &name, e, self_closing);
    }
}

fn handle_end(st: &mut State, name: &str, self_closing: bool) {
    if !self_closing {
        // Close in renderer first (while still in_body) then pop.
        if st.in_body() {
            render_close(st, name);
        } else if st.in_annotation() {
            render_annotation_close(st, name);
        }
    }

    // Pop matching stack entry. We assume well-formed XML — quick-xml errors
    // out on mismatched tags above.
    if let Some(pos) = st.stack.iter().rposition(|s| s == name) {
        st.stack.truncate(pos);
    }

    // Handle metadata side-effects on close.
    match name {
        "binary" => {
            if let (Some(id), Some(mime)) =
                (st.current_binary_id.take(), st.current_binary_mime.take())
            {
                let cleaned: String = st
                    .current_binary_b64
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect();
                if let Ok(data) = base64::engine::general_purpose::STANDARD.decode(cleaned.as_bytes())
                {
                    st.binaries.insert(id, Binary { mime, data });
                }
            }
            st.current_binary_b64.clear();
        }
        "coverpage" => {
            st.in_coverpage = false;
        }
        "author" if st.in_title_info() => {
            let mut full = String::new();
            for part in [&st.author_last, &st.author_first, &st.author_middle] {
                let t = part.trim();
                if !t.is_empty() {
                    if !full.is_empty() {
                        full.push(' ');
                    }
                    full.push_str(t);
                }
            }
            if !full.is_empty() {
                st.authors.push(full);
            }
            st.author_first.clear();
            st.author_middle.clear();
            st.author_last.clear();
        }
        "body" => {
            if st.body_count > 1 {
                st.body_html.push_str("\n</section>\n");
            }
        }
        _ => {}
    }
}

fn handle_text(st: &mut State, text: &str) {
    if st.in_binary() {
        st.current_binary_b64.push_str(text);
        return;
    }

    // Title-info metadata pickers — match on parent tag.
    if st.in_title_info() {
        match st.parent() {
            Some("book-title") if st.title.is_none() => {
                let t = text.trim();
                if !t.is_empty() {
                    st.title = Some(t.to_string());
                }
            }
            Some("lang") if st.lang.is_none() => {
                let t = text.trim();
                if !t.is_empty() {
                    st.lang = Some(t.to_string());
                }
            }
            _ => {}
        }
        if st.in_author() {
            match st.parent() {
                Some("first-name") => st.author_first.push_str(text),
                Some("middle-name") => st.author_middle.push_str(text),
                Some("last-name") => st.author_last.push_str(text),
                _ => {}
            }
        }
    }

    if st.in_body() {
        // Skip body text directly outside any element (whitespace between tags).
        st.body_html.push_str(&xml_escape(text));
    } else if st.in_annotation() {
        st.annotation_html.push_str(&xml_escape(text));
    }
}

// ---------- body / annotation HTML rendering --------------------------------

fn render_open(st: &mut State, name: &str, e: &BytesStart, self_closing: bool) {
    let html = match name {
        "section" => "<section>".to_string(),
        "title" => "<h2>".to_string(),
        "subtitle" => "<h3>".to_string(),
        "p" => "<p>".to_string(),
        "emphasis" => "<em>".to_string(),
        "strong" => "<strong>".to_string(),
        "style" => "<span>".to_string(),
        "sub" => "<sub>".to_string(),
        "sup" => "<sup>".to_string(),
        "epigraph" => "<div class=\"epigraph\">".to_string(),
        "text-author" => "<div class=\"text-author\">".to_string(),
        "poem" => "<div class=\"poem\">".to_string(),
        "stanza" => "<div class=\"stanza\">".to_string(),
        "v" => "<p class=\"verse\">".to_string(),
        "cite" => "<blockquote>".to_string(),
        "date" => "<p class=\"date\">".to_string(),
        "table" => "<table>".to_string(),
        "tr" => "<tr>".to_string(),
        "th" => "<th>".to_string(),
        "td" => "<td>".to_string(),
        "a" => {
            let href = href_attr(e).unwrap_or_else(|| "#".into());
            // Local link to a binary id stays internal; section refs map
            // to in-document anchors that may exist if FB2 used `id=` attrs.
            format!("<a href=\"{}\">", xml_escape_attr(&href))
        }
        "empty-line" => "<div class=\"empty-line\">&#160;</div>".to_string(),
        "image" => {
            if let Some(href) = href_attr(e) {
                let id = href.trim_start_matches('#');
                let src = format!("{}", binary_filename_hint(id));
                format!("<img src=\"{}\" alt=\"\"/>", xml_escape_attr(&src))
            } else {
                String::new()
            }
        }
        "body" if st.body_count == 1 => String::new(),
        // Tags we don't translate: emit nothing on open. Their text content
        // will still flow through `handle_text` when applicable.
        _ => String::new(),
    };
    st.body_html.push_str(&html);

    if self_closing {
        // The <image/> open already emits a self-closed img; no extra close
        // needed. Same for <empty-line/>. Other rare self-closing tags we
        // simply ignore on the close call.
    }
}

fn render_close(st: &mut State, name: &str) {
    let html = match name {
        "section" => "</section>",
        "title" => "</h2>",
        "subtitle" => "</h3>",
        "p" => "</p>",
        "emphasis" => "</em>",
        "strong" => "</strong>",
        "style" => "</span>",
        "sub" => "</sub>",
        "sup" => "</sup>",
        "epigraph" | "text-author" | "poem" | "stanza" => "</div>",
        "v" => "</p>",
        "cite" => "</blockquote>",
        "date" => "</p>",
        "table" => "</table>",
        "tr" => "</tr>",
        "th" => "</th>",
        "td" => "</td>",
        "a" => "</a>",
        _ => "",
    };
    st.body_html.push_str(html);
}

fn render_annotation_open(st: &mut State, name: &str, _e: &BytesStart, _self_closing: bool) {
    match name {
        "p" => st.annotation_html.push_str("<p>"),
        "emphasis" => st.annotation_html.push_str("<em>"),
        "strong" => st.annotation_html.push_str("<strong>"),
        "empty-line" => st.annotation_html.push_str("<br/>"),
        _ => {}
    }
}

fn render_annotation_close(st: &mut State, name: &str) {
    match name {
        "p" => st.annotation_html.push_str("</p>"),
        "emphasis" => st.annotation_html.push_str("</em>"),
        "strong" => st.annotation_html.push_str("</strong>"),
        _ => {}
    }
}

// ---------- EPUB assembly ---------------------------------------------------

const CONTAINER_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
"#;

const DEFAULT_CSS: &str = r#"body { font-family: serif; line-height: 1.5; margin: 1em; }
h1, h2, h3 { font-family: sans-serif; }
img { max-width: 100%; height: auto; }
.epigraph { margin: 1em 2em; font-style: italic; }
.poem { margin-left: 2em; }
.stanza { margin-bottom: 1em; }
.verse { margin: 0; }
.empty-line { height: 1em; }
.text-author { text-align: right; font-style: italic; margin-top: .5em; }
.authors { color: #555; font-style: italic; }
.cover { text-align: center; margin: 1em 0; }
.annotation { border-left: 3px solid #ccc; padding-left: 1em; margin: 1em 0; }
.body-divider { margin: 2em 0; border: 0; border-top: 1px solid #ccc; }
table { border-collapse: collapse; }
th, td { border: 1px solid #999; padding: 4px 8px; }
"#;

fn assemble_epub(p: &Parsed, identifier: &str) -> Result<Vec<u8>> {
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut cursor);

        // mimetype MUST be the first entry, stored uncompressed.
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("mimetype", stored)
            .map_err(|e| Error::Other(format!("epub zip: {e}")))?;
        zip.write_all(b"application/epub+zip")?;

        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        zip.start_file("META-INF/container.xml", deflated)
            .map_err(|e| Error::Other(format!("epub zip: {e}")))?;
        zip.write_all(CONTAINER_XML.as_bytes())?;

        let book_id = format!("urn:mylib:{}", identifier);

        zip.start_file("OEBPS/content.opf", deflated)
            .map_err(|e| Error::Other(format!("epub zip: {e}")))?;
        zip.write_all(build_opf(p, &book_id).as_bytes())?;

        zip.start_file("OEBPS/nav.xhtml", deflated)
            .map_err(|e| Error::Other(format!("epub zip: {e}")))?;
        zip.write_all(build_nav(p).as_bytes())?;

        zip.start_file("OEBPS/toc.ncx", deflated)
            .map_err(|e| Error::Other(format!("epub zip: {e}")))?;
        zip.write_all(build_ncx(p, &book_id).as_bytes())?;

        zip.start_file("OEBPS/style.css", deflated)
            .map_err(|e| Error::Other(format!("epub zip: {e}")))?;
        zip.write_all(DEFAULT_CSS.as_bytes())?;

        zip.start_file("OEBPS/book.xhtml", deflated)
            .map_err(|e| Error::Other(format!("epub zip: {e}")))?;
        zip.write_all(build_book_xhtml(p).as_bytes())?;

        for (id, b) in &p.binaries {
            let name = format!("OEBPS/{}", binary_filename_hint(id));
            zip.start_file(name, deflated)
                .map_err(|e| Error::Other(format!("epub zip: {e}")))?;
            zip.write_all(&b.data)?;
        }

        zip.finish()
            .map_err(|e| Error::Other(format!("epub finish: {e}")))?;
    }
    Ok(cursor.into_inner())
}

fn build_opf(p: &Parsed, book_id: &str) -> String {
    let mut creators = String::new();
    for (i, a) in p.authors.iter().enumerate() {
        creators.push_str(&format!(
            "    <dc:creator id=\"creator{i}\">{}</dc:creator>\n",
            xml_escape(a)
        ));
    }

    let mut manifest = String::new();
    manifest.push_str(
        "    <item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n",
    );
    manifest.push_str("    <item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>\n");
    manifest.push_str("    <item id=\"css\" href=\"style.css\" media-type=\"text/css\"/>\n");
    manifest.push_str(
        "    <item id=\"book\" href=\"book.xhtml\" media-type=\"application/xhtml+xml\"/>\n",
    );

    let mut cover_meta = String::new();
    for (id, b) in &p.binaries {
        let fname = binary_filename_hint(id);
        let item_id = manifest_item_id(id);
        let is_cover = p.cover_id.as_deref() == Some(id.as_str());
        let props = if is_cover {
            " properties=\"cover-image\""
        } else {
            ""
        };
        manifest.push_str(&format!(
            "    <item id=\"{}\" href=\"{}\" media-type=\"{}\"{}/>\n",
            xml_escape_attr(&item_id),
            xml_escape_attr(&fname),
            xml_escape_attr(&b.mime),
            props
        ));
        if is_cover {
            cover_meta = format!(
                "    <meta name=\"cover\" content=\"{}\"/>\n",
                xml_escape_attr(&item_id)
            );
        }
    }

    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid" xml:lang="{lang}">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="bookid">{book_id}</dc:identifier>
    <dc:title>{title}</dc:title>
    <dc:language>{lang}</dc:language>
{creators}{cover_meta}    <meta property="dcterms:modified">2026-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
{manifest}  </manifest>
  <spine toc="ncx">
    <itemref idref="book"/>
  </spine>
</package>
"#,
        lang = xml_escape(&p.lang),
        book_id = xml_escape(book_id),
        title = xml_escape(&p.title),
        creators = creators,
        cover_meta = cover_meta,
        manifest = manifest,
    )
}

fn build_ncx(p: &Parsed, book_id: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head>
    <meta name="dtb:uid" content="{book_id}"/>
    <meta name="dtb:depth" content="1"/>
    <meta name="dtb:totalPageCount" content="0"/>
    <meta name="dtb:maxPageNumber" content="0"/>
  </head>
  <docTitle><text>{title}</text></docTitle>
  <navMap>
    <navPoint id="navPoint-1" playOrder="1">
      <navLabel><text>Книга</text></navLabel>
      <content src="book.xhtml"/>
    </navPoint>
  </navMap>
</ncx>
"#,
        book_id = xml_escape(book_id),
        title = xml_escape(&p.title),
    )
}

fn build_nav(p: &Parsed) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" lang="{lang}">
<head><title>Оглавление</title><meta charset="UTF-8"/></head>
<body>
  <nav epub:type="toc">
    <h1>Оглавление</h1>
    <ol><li><a href="book.xhtml">{title}</a></li></ol>
  </nav>
</body>
</html>
"#,
        lang = xml_escape(&p.lang),
        title = xml_escape(&p.title),
    )
}

fn build_book_xhtml(p: &Parsed) -> String {
    let mut s = String::with_capacity(p.body_html.len() + 512);
    s.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" lang=""#);
    s.push_str(&xml_escape_attr(&p.lang));
    s.push_str(r#"">
<head>
<title>"#);
    s.push_str(&xml_escape(&p.title));
    s.push_str(r#"</title>
<meta charset="UTF-8"/>
<link rel="stylesheet" type="text/css" href="style.css"/>
</head>
<body>
"#);
    s.push_str(&format!("<h1>{}</h1>\n", xml_escape(&p.title)));
    if !p.authors.is_empty() {
        s.push_str(&format!(
            "<p class=\"authors\">{}</p>\n",
            xml_escape(&p.authors.join(", "))
        ));
    }
    if let Some(cover_id) = &p.cover_id {
        if p.binaries.contains_key(cover_id) {
            s.push_str(&format!(
                "<div class=\"cover\"><img src=\"{}\" alt=\"\"/></div>\n",
                xml_escape_attr(&binary_filename_hint(cover_id))
            ));
        }
    }
    if !p.annotation_html.trim().is_empty() {
        s.push_str("<div class=\"annotation\">\n");
        s.push_str(&p.annotation_html);
        s.push_str("\n</div>\n");
    }
    s.push_str(&p.body_html);
    s.push_str("\n</body>\n</html>\n");
    s
}

// ---------- helpers ---------------------------------------------------------

fn attr(e: &BytesStart, key: &str) -> Option<String> {
    for a in e.attributes().flatten() {
        let k = std::str::from_utf8(a.key.as_ref()).unwrap_or("");
        if k.eq_ignore_ascii_case(key) {
            return std::str::from_utf8(a.value.as_ref()).ok().map(str::to_string);
        }
    }
    None
}

fn href_attr(e: &BytesStart) -> Option<String> {
    attr(e, "l:href")
        .or_else(|| attr(e, "xlink:href"))
        .or_else(|| attr(e, "href"))
}

fn local_name(name: &[u8]) -> String {
    let s = std::str::from_utf8(name).unwrap_or("");
    match s.find(':') {
        Some(p) => s[p + 1..].to_ascii_lowercase(),
        None => s.to_ascii_lowercase(),
    }
}

fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

fn xml_escape_attr(s: &str) -> String {
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

/// FB2 binary ids look like filenames already (e.g. "cover.jpg", "img001.png").
/// We sanitize a few forbidden characters and keep the rest so the reference
/// in body XHTML matches the file inside OEBPS.
fn binary_filename_hint(id: &str) -> String {
    let mut out = String::with_capacity(id.len());
    for c in id.chars() {
        match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | ' ' => out.push('_'),
            c if (c as u32) < 0x20 => out.push('_'),
            c => out.push(c),
        }
    }
    if out.is_empty() {
        "asset.bin".into()
    } else {
        out
    }
}

fn manifest_item_id(raw: &str) -> String {
    // EPUB OPF item ids must start with a letter and only contain
    // [A-Za-z0-9._-] — derive a safe id from the binary name.
    let mut out = String::with_capacity(raw.len() + 4);
    out.push_str("img-");
    for c in raw.chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

fn sanitize_stem(title: &str) -> String {
    let t = title.trim();
    let mut out = String::with_capacity(t.len());
    for c in t.chars() {
        match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => out.push('_'),
            c if (c as u32) < 0x20 => out.push('_'),
            c => out.push(c),
        }
    }
    let trimmed = out.trim().trim_matches('.').to_string();
    if trimmed.is_empty() {
        "book".into()
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

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
  <description>
    <title-info>
      <author><first-name>Stephen</first-name><last-name>King</last-name></author>
      <book-title>It</book-title>
      <annotation><p>Clown horror.</p></annotation>
      <coverpage><image l:href="#cover.jpg"/></coverpage>
      <lang>en</lang>
    </title-info>
  </description>
  <body>
    <section>
      <title><p>Chapter 1</p></title>
      <p>First <emphasis>line</emphasis>.</p>
      <empty-line/>
      <p>Second paragraph with <a l:href="#fn1">ref</a>.</p>
    </section>
  </body>
  <body name="notes">
    <section id="fn1"><p>A note.</p></section>
  </body>
  <binary id="cover.jpg" content-type="image/jpeg">SGVsbG8=</binary>
</FictionBook>"##;

    #[test]
    fn parses_metadata_and_body() {
        let p = parse(SAMPLE.as_bytes()).unwrap();
        assert_eq!(p.title, "It");
        assert_eq!(p.authors, vec!["King Stephen".to_string()]);
        assert_eq!(p.lang, "en");
        assert!(p.body_html.contains("<h2>"));
        assert!(p.body_html.contains("<em>line</em>"));
        assert!(p.body_html.contains("<div class=\"empty-line\""));
        assert!(p.body_html.contains("body-divider"));
        assert!(p.annotation_html.contains("Clown horror"));
        assert_eq!(p.cover_id.as_deref(), Some("cover.jpg"));
        assert!(p.binaries.contains_key("cover.jpg"));
    }

    #[test]
    fn produces_valid_epub_structure() {
        let r = convert_fb2_to_epub(SAMPLE.as_bytes(), "test-1").unwrap();
        // Should be a non-empty ZIP starting with "PK".
        assert!(r.bytes.len() > 200);
        assert_eq!(&r.bytes[0..2], b"PK");
        // mimetype entry must be first and uncompressed — easiest way to
        // check is to crack open the archive.
        let cur = Cursor::new(r.bytes.clone());
        let mut zip = zip::ZipArchive::new(cur).unwrap();
        let mut names: Vec<String> = (0..zip.len())
            .map(|i| zip.by_index(i).unwrap().name().to_string())
            .collect();
        assert_eq!(names.remove(0), "mimetype");
        assert!(names.iter().any(|n| n == "META-INF/container.xml"));
        assert!(names.iter().any(|n| n == "OEBPS/content.opf"));
        assert!(names.iter().any(|n| n == "OEBPS/book.xhtml"));
        assert!(names.iter().any(|n| n == "OEBPS/cover.jpg"));
    }
}
