use std::io::Read;
use std::path::Path;

use base64::Engine;
use quick_xml::events::Event;
use quick_xml::reader::Reader;

use crate::error::{Error, Result};
use crate::model::BookContent;

/// Open the zip archive at `zip_path`, locate the book file (`<file>.<ext>`)
/// inside it, and pull out a short description plus the cover image (if any)
/// for the card view.
pub fn read_book_content(zip_path: &Path, file: &str, ext: &str) -> Result<BookContent> {
    if !zip_path.exists() {
        return Err(Error::NotFound(format!(
            "архив не найден: {}",
            zip_path.display()
        )));
    }
    let zf = std::fs::File::open(zip_path)?;
    let mut zip = zip::ZipArchive::new(zf)?;

    let candidates = [
        format!("{file}.{ext}"),
        format!("{file}.{}", ext.to_ascii_lowercase()),
        format!("{file}.{}", ext.to_ascii_uppercase()),
        format!("{file}.fb2"),
    ];

    let mut buf: Vec<u8> = Vec::new();
    let mut found = None;
    for name in &candidates {
        if let Ok(mut entry) = zip.by_name(name) {
            entry.read_to_end(&mut buf)?;
            found = Some(name.clone());
            break;
        }
    }
    let source = match found {
        Some(n) => format!("{}!{n}", zip_path.display()),
        None => {
            return Err(Error::NotFound(format!(
                "файл книги не найден в {}",
                zip_path.display()
            )))
        }
    };

    if ext.eq_ignore_ascii_case("fb2") {
        let mut content = parse_fb2(&buf)?;
        content.source = source;
        Ok(content)
    } else {
        Ok(BookContent {
            description: format!("Формат {} — превью пока не поддерживается.", ext.to_uppercase()),
            cover_data_url: None,
            source,
        })
    }
}

#[derive(Default)]
struct ParseState {
    in_annotation: bool,
    annotation_depth: i32,
    annotation_buf: String,

    in_coverpage: bool,
    cover_id: Option<String>,

    in_binary: bool,
    current_binary_id: Option<String>,
    current_binary_mime: Option<String>,
    current_binary_data: String,
    cover_mime: Option<String>,
    cover_b64: Option<String>,
}

/// Extract just the cover image from an FB2 byte slice. Returns
/// `(bytes, mime)` if a `<coverpage>` is present and the referenced
/// `<binary>` decodes; `None` otherwise. Faster than `parse_fb2` because it
/// stops as soon as the cover binary is read and skips the annotation.
pub fn extract_cover(bytes: &[u8]) -> Result<Option<(Vec<u8>, String)>> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    reader.config_mut().expand_empty_elements = false;

    let mut buf = Vec::new();
    let mut in_coverpage = false;
    let mut cover_id: Option<String> = None;

    let mut in_binary = false;
    let mut cur_id: Option<String> = None;
    let mut cur_mime: Option<String> = None;
    let mut cur_b64 = String::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => return Err(Error::Xml(e.into())),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = local_name(e.name().as_ref());
                match name.as_str() {
                    "coverpage" => in_coverpage = true,
                    "binary" => {
                        in_binary = true;
                        cur_id = attr(&e, "id");
                        cur_mime = attr(&e, "content-type")
                            .or_else(|| Some("image/jpeg".into()));
                        cur_b64.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let name = local_name(e.name().as_ref());
                if name == "image" && in_coverpage && cover_id.is_none() {
                    if let Some(href) = attr(&e, "l:href")
                        .or_else(|| attr(&e, "xlink:href"))
                        .or_else(|| attr(&e, "href"))
                    {
                        cover_id = Some(href.trim_start_matches('#').to_string());
                    }
                }
            }
            Ok(Event::Text(t)) if in_binary => {
                let s = t.unescape().map_err(|e| Error::Xml(e.into()))?;
                cur_b64.push_str(&s);
            }
            Ok(Event::CData(c)) if in_binary => {
                let s = std::str::from_utf8(c.as_ref()).unwrap_or_default();
                cur_b64.push_str(s);
            }
            Ok(Event::End(e)) => {
                let name = local_name(e.name().as_ref());
                match name.as_str() {
                    "coverpage" => in_coverpage = false,
                    "binary" => {
                        if let (Some(id), Some(mime)) = (cur_id.take(), cur_mime.take()) {
                            if Some(&id) == cover_id.as_ref() {
                                let cleaned: String =
                                    cur_b64.chars().filter(|c| !c.is_whitespace()).collect();
                                if let Ok(decoded) = base64::engine::general_purpose::STANDARD
                                    .decode(cleaned.as_bytes())
                                {
                                    return Ok(Some((decoded, mime)));
                                }
                                return Ok(None);
                            }
                        }
                        in_binary = false;
                        cur_b64.clear();
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        buf.clear();
    }
    Ok(None)
}

fn parse_fb2(bytes: &[u8]) -> Result<BookContent> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    reader.config_mut().expand_empty_elements = false;

    let mut state = ParseState::default();
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Err(e) => return Err(Error::Xml(e.into())),
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = local_name(e.name().as_ref());
                match name.as_str() {
                    "annotation" => {
                        state.in_annotation = true;
                        state.annotation_depth = 1;
                    }
                    "coverpage" => {
                        state.in_coverpage = true;
                    }
                    "binary" => {
                        state.in_binary = true;
                        state.current_binary_data.clear();
                        state.current_binary_id = attr(&e, "id");
                        state.current_binary_mime =
                            attr(&e, "content-type").or_else(|| Some("image/jpeg".into()));
                    }
                    "p" | "v" | "subtitle" | "cite" if state.in_annotation => {
                        state.annotation_depth += 1;
                    }
                    "empty-line" if state.in_annotation => {
                        state.annotation_buf.push('\n');
                    }
                    _ if state.in_annotation => {
                        state.annotation_depth += 1;
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(e)) => {
                let name = local_name(e.name().as_ref());
                if name == "image" && state.in_coverpage && state.cover_id.is_none() {
                    if let Some(href) = attr(&e, "l:href")
                        .or_else(|| attr(&e, "xlink:href"))
                        .or_else(|| attr(&e, "href"))
                    {
                        state.cover_id = Some(href.trim_start_matches('#').to_string());
                    }
                }
                if name == "empty-line" && state.in_annotation {
                    state.annotation_buf.push('\n');
                }
            }
            Ok(Event::Text(t)) => {
                if state.in_annotation {
                    let text = t.unescape().map_err(|e| Error::Xml(e.into()))?;
                    state.annotation_buf.push_str(&text);
                } else if state.in_binary {
                    state
                        .current_binary_data
                        .push_str(t.unescape().map_err(|e| Error::Xml(e.into()))?.as_ref());
                }
            }
            Ok(Event::CData(c)) => {
                if state.in_binary {
                    let s = std::str::from_utf8(c.as_ref()).unwrap_or_default();
                    state.current_binary_data.push_str(s);
                }
            }
            Ok(Event::End(e)) => {
                let name = local_name(e.name().as_ref());
                match name.as_str() {
                    "annotation" => {
                        state.annotation_depth -= 1;
                        if state.annotation_depth <= 0 {
                            state.in_annotation = false;
                            if !state.annotation_buf.is_empty() {
                                state.annotation_buf.push('\n');
                            }
                        }
                    }
                    "coverpage" => state.in_coverpage = false,
                    "binary" => {
                        if let Some(ref id) = state.current_binary_id {
                            if Some(id) == state.cover_id.as_ref() {
                                state.cover_mime = state.current_binary_mime.take();
                                state.cover_b64 =
                                    Some(state.current_binary_data.replace(|c: char| c.is_whitespace(), ""));
                            }
                        }
                        state.in_binary = false;
                        state.current_binary_data.clear();
                        state.current_binary_id = None;
                        state.current_binary_mime = None;
                    }
                    "p" | "v" | "subtitle" | "cite" if state.in_annotation => {
                        state.annotation_depth -= 1;
                        state.annotation_buf.push('\n');
                    }
                    _ if state.in_annotation => {
                        state.annotation_depth -= 1;
                    }
                    _ => {}
                }
                if !state.in_annotation
                    && !state.in_binary
                    && state.cover_b64.is_some()
                    && !state.annotation_buf.is_empty()
                {
                    break;
                }
            }
            _ => {}
        }
        buf.clear();
    }

    let description = collapse_whitespace(&state.annotation_buf);
    let cover_data_url = state.cover_b64.as_ref().map(|b64| {
        let mime = state.cover_mime.as_deref().unwrap_or("image/jpeg");
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(b64.as_bytes())
            .unwrap_or_default();
        let clean = base64::engine::general_purpose::STANDARD.encode(bytes);
        format!("data:{mime};base64,{clean}")
    });

    Ok(BookContent {
        description,
        cover_data_url,
        source: String::new(),
    })
}

fn attr(e: &quick_xml::events::BytesStart, key: &str) -> Option<String> {
    for a in e.attributes().flatten() {
        let k = std::str::from_utf8(a.key.as_ref()).unwrap_or("");
        if k.eq_ignore_ascii_case(key) {
            return std::str::from_utf8(a.value.as_ref())
                .ok()
                .map(str::to_string);
        }
    }
    None
}

fn local_name(name: &[u8]) -> String {
    let s = std::str::from_utf8(name).unwrap_or("");
    if let Some(pos) = s.find(':') {
        s[pos + 1..].to_ascii_lowercase()
    } else {
        s.to_ascii_lowercase()
    }
}

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_nl = 0;
    let mut prev_space = false;
    for ch in s.chars() {
        if ch == '\n' {
            prev_nl += 1;
            prev_space = false;
            if prev_nl <= 2 {
                out.push('\n');
            }
        } else if ch.is_whitespace() {
            if !prev_space && !out.ends_with('\n') {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(ch);
            prev_nl = 0;
            prev_space = false;
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
  <description>
    <title-info>
      <annotation>
        <p>This is the <emphasis>annotation</emphasis>.</p>
        <p>Second paragraph here.</p>
      </annotation>
      <coverpage><image l:href="#cover.jpg"/></coverpage>
    </title-info>
  </description>
  <body><section><p>body</p></section></body>
  <binary id="cover.jpg" content-type="image/jpeg">SGVsbG8=</binary>
</FictionBook>"##;

    #[test]
    fn extracts_annotation_and_cover() {
        let r = parse_fb2(SAMPLE.as_bytes()).unwrap();
        assert!(r.description.contains("annotation"), "got: {:?}", r.description);
        assert!(r.description.contains("Second paragraph"));
        let url = r.cover_data_url.expect("cover should be present");
        assert!(url.starts_with("data:image/jpeg;base64,"));
        assert!(url.ends_with("SGVsbG8="));
    }
}
