use std::io::Read;
use std::path::Path;

use encoding_rs::{UTF_8, WINDOWS_1251};

use crate::error::Result;
use crate::model::{AuthorName, InpRecord};

/// Default field order used by MyHomeLib / Flibusta INPX when no structure.info
/// is provided.
const DEFAULT_STRUCTURE: &[&str] = &[
    "AUTHOR", "GENRE", "TITLE", "SERIES", "SERNO", "FILE", "SIZE", "LIBID", "DEL", "EXT", "DATE",
    "LANG", "LIBRATE", "KEYWORDS",
];

/// Records inside .inp files use 0x04 (EOT) to separate fields and 0x0A to
/// separate records. Older catalogs sometimes use TAB — we accept both.
const FIELD_SEPS: &[u8] = &[0x04, b'\t'];

#[derive(Debug, Clone, Default)]
pub struct InpxMetadata {
    pub collection_name: String,
    pub collection_version: String,
    pub structure: Vec<String>,
}

pub fn read_metadata(path: &Path) -> Result<InpxMetadata> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut meta = InpxMetadata::default();

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_ascii_lowercase();
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        match name.as_str() {
            "collection.info" => {
                meta.collection_name = decode_text(&buf).lines().next().unwrap_or("").to_string();
            }
            "version.info" => {
                meta.collection_version =
                    decode_text(&buf).lines().next().unwrap_or("").to_string();
            }
            "structure.info" => {
                let text = decode_text(&buf);
                meta.structure = text
                    .split(|c| c == ';' || c == '\n' || c == '\r')
                    .map(|s| s.trim().to_ascii_uppercase())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            _ => {}
        }
    }
    Ok(meta)
}

/// Sum of uncompressed sizes of every `.inp` entry — a stable signal for the
/// "total work" of an import. Cheap: just iterates the zip central directory
/// without decompressing anything.
pub fn compute_inp_byte_total(path: &Path) -> Result<u64> {
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    let mut total: u64 = 0;
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        if entry.name().to_ascii_lowercase().ends_with(".inp") {
            total = total.saturating_add(entry.size());
        }
    }
    Ok(total)
}

/// Iterate every `*.inp` entry in the archive and yield parsed records.
/// `on_record` receives each record; `on_inp_done` fires after each `.inp`
/// entry has been fully processed, with its uncompressed byte count — use it
/// to drive a progress bar. Returning `Err` from either callback stops the
/// iteration.
pub fn parse_inpx<F, P>(path: &Path, mut on_record: F, mut on_inp_done: P) -> Result<()>
where
    F: FnMut(InpRecord) -> Result<()>,
    P: FnMut(u64),
{
    let file = std::fs::File::open(path)?;
    let mut archive = zip::ZipArchive::new(file)?;

    // Pull structure.info first if present, so we know the field layout.
    let mut structure_owned: Option<Vec<String>> = None;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        if entry.name().eq_ignore_ascii_case("structure.info") {
            let mut buf = Vec::new();
            entry.read_to_end(&mut buf)?;
            let text = decode_text(&buf);
            structure_owned = Some(
                text.split(|c| c == ';' || c == '\n' || c == '\r')
                    .map(|s| s.trim().to_ascii_uppercase())
                    .filter(|s| !s.is_empty())
                    .collect(),
            );
            break;
        }
    }
    let structure: Vec<String> = structure_owned.unwrap_or_else(|| {
        DEFAULT_STRUCTURE
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    });

    for i in 0..archive.len() {
        let (entry_name, entry_size) = {
            let entry = archive.by_index(i)?;
            (entry.name().to_string(), entry.size())
        };
        if !entry_name.to_ascii_lowercase().ends_with(".inp") {
            continue;
        }
        let archive_base = entry_name
            .strip_suffix(".inp")
            .or_else(|| entry_name.strip_suffix(".INP"))
            .unwrap_or(&entry_name)
            .to_string();

        let mut entry = archive.by_index(i)?;
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf)?;
        let text = decode_text(&buf);
        parse_inp_text(&text, &structure, &archive_base, &mut on_record)?;
        on_inp_done(entry_size);
    }
    Ok(())
}

fn parse_inp_text<F>(
    text: &str,
    structure: &[String],
    archive_base: &str,
    on_record: &mut F,
) -> Result<()>
where
    F: FnMut(InpRecord) -> Result<()>,
{
    for raw_line in text.split('\n') {
        let line = raw_line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = split_record(line);
        let rec = build_record(&fields, structure, archive_base);
        if rec.title.is_empty() && rec.file.is_empty() && rec.authors.is_empty() {
            continue;
        }
        on_record(rec)?;
    }
    Ok(())
}

fn split_record(line: &str) -> Vec<&str> {
    // Detect which separator this line uses (some catalogs mix).
    let bytes = line.as_bytes();
    let sep = FIELD_SEPS
        .iter()
        .copied()
        .find(|&b| bytes.contains(&b))
        .unwrap_or(0x04);
    line.split(sep as char).collect()
}

fn build_record(fields: &[&str], structure: &[String], archive_base: &str) -> InpRecord {
    let mut rec = InpRecord::default();
    rec.archive = format!("{archive_base}.zip");

    for (i, key) in structure.iter().enumerate() {
        let v = fields.get(i).copied().unwrap_or("").trim();
        match key.as_str() {
            "AUTHOR" => rec.authors = parse_authors(v),
            "GENRE" => rec.genres = parse_list(v),
            "TITLE" => rec.title = v.to_string(),
            "SERIES" => rec.series = v.to_string(),
            "SERNO" => rec.ser_no = v.parse::<u32>().ok(),
            "FILE" => rec.file = v.to_string(),
            "SIZE" => rec.size = v.parse::<u64>().unwrap_or(0),
            "LIBID" => rec.lib_id = v.to_string(),
            "DEL" => rec.deleted = matches!(v, "1" | "Y" | "y" | "true"),
            "EXT" => rec.ext = if v.is_empty() { "fb2".into() } else { v.into() },
            "DATE" => rec.date = v.to_string(),
            "LANG" => rec.lang = v.to_string(),
            "LIBRATE" => rec.librate = v.parse::<u32>().ok(),
            "KEYWORDS" => rec.keywords = v.to_string(),
            "FOLDER" | "INSNO" => { /* not stored; FOLDER could override archive — rare */ }
            _ => {}
        }
    }
    if rec.ext.is_empty() {
        rec.ext = "fb2".into();
    }
    rec
}

fn parse_authors(field: &str) -> Vec<AuthorName> {
    field
        .split(':')
        .filter(|s| !s.trim().is_empty())
        .map(|chunk| {
            let parts: Vec<&str> = chunk.split(',').map(str::trim).collect();
            AuthorName {
                last: parts.first().copied().unwrap_or("").to_string(),
                first: parts.get(1).copied().unwrap_or("").to_string(),
                middle: parts.get(2).copied().unwrap_or("").to_string(),
            }
        })
        .filter(|a| !a.is_empty())
        .collect()
}

fn parse_list(field: &str) -> Vec<String> {
    field
        .split(':')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Decode bytes as UTF-8 with replacement for invalid sequences. If the byte
/// sequence looks like CP1251 (lots of high-bit bytes that aren't valid UTF-8),
/// decode as CP1251 instead. Most modern INPX archives are UTF-8.
pub fn decode_text(bytes: &[u8]) -> String {
    let (text, _, had_errors) = UTF_8.decode(bytes);
    if !had_errors {
        return text.into_owned();
    }
    let (text, _, _) = WINDOWS_1251.decode(bytes);
    text.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_authors_genres_and_basic_fields() {
        let mut got = Vec::new();
        let structure: Vec<String> = DEFAULT_STRUCTURE.iter().map(|s| s.to_string()).collect();
        let line = "Tolkien,John,Ronald:\u{4}fantasy:adventure:\u{4}The Hobbit\u{4}Middle Earth\u{4}1\u{4}123\u{4}500000\u{4}LIB42\u{4}0\u{4}fb2\u{4}1937-01-01\u{4}en\u{4}5\u{4}";
        parse_inp_text(line, &structure, "fb.test.001-100", &mut |r| {
            got.push(r);
            Ok(())
        })
        .unwrap();
        assert_eq!(got.len(), 1);
        let r = &got[0];
        assert_eq!(r.title, "The Hobbit");
        assert_eq!(r.authors.len(), 1);
        assert_eq!(r.authors[0].last, "Tolkien");
        assert_eq!(r.authors[0].first, "John");
        assert_eq!(r.authors[0].middle, "Ronald");
        assert_eq!(r.genres, vec!["fantasy", "adventure"]);
        assert_eq!(r.series, "Middle Earth");
        assert_eq!(r.ser_no, Some(1));
        assert_eq!(r.file, "123");
        assert_eq!(r.size, 500000);
        assert_eq!(r.archive, "fb.test.001-100.zip");
        assert_eq!(r.ext, "fb2");
        assert_eq!(r.lang, "en");
    }

    #[test]
    fn falls_back_to_tab_separator() {
        let mut got = Vec::new();
        let structure: Vec<String> = DEFAULT_STRUCTURE.iter().map(|s| s.to_string()).collect();
        let line = "Doe,Jane,\tnovel:\tTitle\t\t\t10\t100\tID1\t0\tfb2\t\ten\t\t";
        parse_inp_text(line, &structure, "x.001-002", &mut |r| {
            got.push(r);
            Ok(())
        })
        .unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].title, "Title");
        assert_eq!(got[0].authors[0].last, "Doe");
    }
}
