use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::model::Book;

const MAX_NAME_BYTES: usize = 200;
const FORBIDDEN: &[char] = &['/', '\\', ':', '*', '?', '"', '<', '>', '|'];

/// Build the per-book target path inside `<root>` following the
/// `Author/Series/File` layout. When a book sits in a series, the file gets
/// a zero-padded number prefix so the filesystem keeps the reading order.
pub fn target_path_for(root: &Path, book: &Book) -> PathBuf {
    let author = book
        .authors
        .first()
        .map(|a| a.display())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "Без автора".to_string());

    let mut dir = root.join(sanitize(&author));
    if let Some(series) = book.series.as_ref().filter(|s| !s.trim().is_empty()) {
        dir = dir.join(sanitize(series));
    }

    // Only books with a real, non-zero `ser_no` inside a series get the
    // number prefix — books that share a series but were never numbered (or
    // arrived as SERNO=0) keep the bare title.
    let series_num = book
        .ser_no
        .filter(|&n| n > 0)
        .filter(|_| book.series.is_some());
    let stem = match series_num {
        Some(no) => format!("{:02} {}", no, sanitize(&book.title)),
        None if !book.title.is_empty() => sanitize(&book.title),
        None => sanitize(&book.file),
    };

    let ext = if book.ext.is_empty() {
        "fb2".to_string()
    } else {
        sanitize(&book.ext)
    };

    dir.join(format!("{stem}.{ext}"))
}

/// Copy a single book file from its companion `.zip` to `target`. The caller
/// is responsible for creating parent directories. Returns `Ok(false)` if the
/// target file already exists with the same byte length — we treat that as
/// "already exported" and skip silently.
pub fn copy_book_from_zip(zip_path: &Path, file: &str, ext: &str, target: &Path) -> Result<bool> {
    let zf = std::fs::File::open(zip_path)?;
    let mut zip = zip::ZipArchive::new(zf)?;

    let candidates = [
        format!("{file}.{ext}"),
        format!("{file}.{}", ext.to_ascii_lowercase()),
        format!("{file}.{}", ext.to_ascii_uppercase()),
        format!("{file}.fb2"),
    ];

    let mut found_name = None;
    let mut entry_size = 0u64;
    for name in &candidates {
        if let Ok(entry) = zip.by_name(name) {
            entry_size = entry.size();
            found_name = Some(name.clone());
            break;
        }
    }
    let name = found_name.ok_or_else(|| {
        Error::NotFound(format!(
            "файл {file}.{ext} не найден в {}",
            zip_path.display()
        ))
    })?;

    if let Ok(meta) = std::fs::metadata(target) {
        if meta.len() == entry_size {
            return Ok(false);
        }
    }

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Open a fresh entry handle since we read `entry.size()` above.
    let mut entry = zip.by_name(&name)?;
    let final_path = unique_path(target);
    let mut out = std::fs::File::create(&final_path)?;
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = entry.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
    }
    out.flush()?;
    Ok(true)
}

/// If `path` already exists, append " (2)", " (3)", … before the extension
/// until we find a free name. Bounded so we don't spin forever on weird
/// filesystems.
fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_path_buf();
    }
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("book")
        .to_string();
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_string();
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    for i in 2..1000 {
        let name = if ext.is_empty() {
            format!("{stem} ({i})")
        } else {
            format!("{stem} ({i}).{ext}")
        };
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    path.to_path_buf()
}

fn sanitize(s: &str) -> String {
    let mut out: String = s
        .chars()
        .map(|c| {
            if FORBIDDEN.contains(&c) {
                '_'
            } else if (c as u32) < 0x20 || c == '\x7F' {
                '_'
            } else {
                c
            }
        })
        .collect();

    // Trim whitespace + leading/trailing dots (Windows treats trailing dots
    // specially) so the path component is well-formed across all OSes.
    out = out.trim().trim_matches('.').to_string();

    // Truncate by byte length, on a char boundary.
    if out.len() > MAX_NAME_BYTES {
        let mut cut = MAX_NAME_BYTES;
        while !out.is_char_boundary(cut) {
            cut -= 1;
        }
        out.truncate(cut);
        out = out.trim_end().to_string();
    }
    if out.is_empty() {
        out = "_".into();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AuthorName, Book};

    fn book(title: &str, author_last: &str, series: Option<&str>, ser_no: Option<u32>) -> Book {
        Book {
            id: 1,
            title: title.into(),
            authors: vec![AuthorName {
                last: author_last.into(),
                first: String::new(),
                middle: String::new(),
            }],
            genres: vec![],
            series: series.map(str::to_string),
            ser_no,
            file: "f".into(),
            archive: "a.zip".into(),
            size: 1,
            lib_id: "x".into(),
            deleted: false,
            ext: "fb2".into(),
            date: String::new(),
            lang: String::new(),
            librate: None,
        }
    }

    #[test]
    fn path_layout_author_series_file() {
        let p = target_path_for(
            Path::new("/tmp/out"),
            &book("Война и мир", "Толстой", Some("Эпопея"), Some(3)),
        );
        assert_eq!(
            p,
            PathBuf::from("/tmp/out/Толстой/Эпопея/03 Война и мир.fb2")
        );
    }

    #[test]
    fn standalone_book_skips_series_dir_and_number_prefix() {
        let p = target_path_for(
            Path::new("/tmp/out"),
            &book("Севастопольские рассказы", "Толстой", None, None),
        );
        assert_eq!(
            p,
            PathBuf::from("/tmp/out/Толстой/Севастопольские рассказы.fb2")
        );
    }

    #[test]
    fn series_book_without_number_skips_prefix() {
        let p = target_path_for(
            Path::new("/tmp/out"),
            &book("Без номера", "Толстой", Some("Эпопея"), None),
        );
        assert_eq!(p, PathBuf::from("/tmp/out/Толстой/Эпопея/Без номера.fb2"));

        let p_zero = target_path_for(
            Path::new("/tmp/out"),
            &book("С нулём", "Толстой", Some("Эпопея"), Some(0)),
        );
        assert_eq!(
            p_zero,
            PathBuf::from("/tmp/out/Толстой/Эпопея/С нулём.fb2"),
            "ser_no=0 should be treated as 'not numbered'"
        );
    }

    #[test]
    fn sanitize_strips_forbidden_chars() {
        let p = target_path_for(
            Path::new("/tmp"),
            &book("A/B:C", "X*Y", Some("S?D"), Some(1)),
        );
        assert_eq!(p, PathBuf::from("/tmp/X_Y/S_D/01 A_B_C.fb2"));
    }
}
