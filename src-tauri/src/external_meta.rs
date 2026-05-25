//! Best-effort enrichment of book records with public metadata from
//! Google Books and OpenLibrary.
//!
//! Both APIs are queried by `intitle + inauthor` (Google) or `title + author`
//! (OL). Returned snippets are stored as `ExternalMetaEntry` rows in
//! `book_external_meta` keyed by `(lib_id, source)`.
//!
//! All HTTP calls are blocking and capped at a short timeout. Callers are
//! expected to run this from a worker thread (`tauri::async_runtime::spawn_blocking`).

use std::time::Duration;

use serde_json::Value;

use crate::model::ExternalMetaEntry;

const TIMEOUT: Duration = Duration::from_secs(6);
const USER_AGENT: &str = concat!(
    "mylib/",
    env!("CARGO_PKG_VERSION"),
    " (offline FB2/EPUB reader; +https://github.com/legostin)"
);

pub fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(TIMEOUT)
        .timeout_read(TIMEOUT)
        .timeout_write(TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
}

/// Sanitize a free-form text fragment by stripping HTML tags and collapsing
/// runs of whitespace. Both APIs occasionally return small bits of markup in
/// descriptions; we render the result in a plain-text container so we don't
/// want raw tags showing up.
fn clean_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    let mut prev_space = false;
    for ch in s.chars() {
        if in_tag {
            if ch == '>' {
                in_tag = false;
            }
            continue;
        }
        if ch == '<' {
            in_tag = true;
            continue;
        }
        if ch.is_whitespace() {
            if !prev_space && !out.is_empty() {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
    }
    out.trim().to_string()
}

pub fn fetch_google_books(title: &str, author: Option<&str>) -> ExternalMetaEntry {
    let now = now_secs();
    let title = title.trim();
    if title.is_empty() {
        return ExternalMetaEntry {
            source: "google".into(),
            status: "skipped".into(),
            description: None,
            rating: None,
            rating_count: None,
            url: None,
            fetched_at: now,
        };
    }
    let mut q = format!("intitle:{}", title);
    if let Some(a) = author.map(str::trim).filter(|s| !s.is_empty()) {
        q.push_str(&format!("+inauthor:{}", a));
    }
    let url = format!(
        "https://www.googleapis.com/books/v1/volumes?q={}&maxResults=3&printType=books",
        urlencoding::encode(&q)
    );
    match agent().get(&url).call() {
        Ok(resp) => {
            let v: Value = match resp.into_json() {
                Ok(v) => v,
                Err(_) => return error_entry("google", "bad json", now),
            };
            let Some(item) = pick_google_item(&v, title, author) else {
                return ExternalMetaEntry {
                    source: "google".into(),
                    status: "not_found".into(),
                    description: None,
                    rating: None,
                    rating_count: None,
                    url: None,
                    fetched_at: now,
                };
            };
            let info = &item["volumeInfo"];
            let description = info["description"].as_str().map(clean_text).filter(|s| !s.is_empty());
            let rating = info["averageRating"].as_f64();
            let rating_count = info["ratingsCount"].as_i64();
            let url = info["infoLink"]
                .as_str()
                .or_else(|| info["canonicalVolumeLink"].as_str())
                .map(str::to_string);
            ExternalMetaEntry {
                source: "google".into(),
                status: "ok".into(),
                description,
                rating,
                rating_count,
                url,
                fetched_at: now,
            }
        }
        Err(e) => {
            tracing::warn!("google books fetch failed: {e}");
            error_entry("google", "network", now)
        }
    }
}

/// Pick the candidate volume that best matches the input title/author. Google
/// Books often returns audio editions, foreign translations, or unrelated
/// titles in the top slot, so a simple "best-of-3" filter beats blindly taking
/// `items[0]`.
fn pick_google_item<'a>(v: &'a Value, title: &str, author: Option<&str>) -> Option<&'a Value> {
    let items = v["items"].as_array()?;
    let want_title = title.to_lowercase();
    let want_author = author.map(|a| a.to_lowercase());
    let mut best: Option<(i32, &'a Value)> = None;
    for it in items {
        let info = &it["volumeInfo"];
        let cand_title = info["title"].as_str().unwrap_or("").to_lowercase();
        let cand_authors: Vec<String> = info["authors"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                    .collect()
            })
            .unwrap_or_default();
        let mut score = 0i32;
        if cand_title.contains(&want_title) || want_title.contains(&cand_title) {
            score += 2;
        }
        if let Some(wa) = want_author.as_ref() {
            if cand_authors.iter().any(|a| {
                let a = a.to_lowercase();
                a.contains(wa) || wa.contains(&a)
            }) {
                score += 2;
            }
        }
        if info["description"].is_string() {
            score += 1;
        }
        match best {
            Some((s, _)) if s >= score => {}
            _ => best = Some((score, it)),
        }
    }
    best.map(|(_, it)| it)
}

pub fn fetch_openlibrary(title: &str, author: Option<&str>) -> ExternalMetaEntry {
    let now = now_secs();
    let title = title.trim();
    if title.is_empty() {
        return ExternalMetaEntry {
            source: "openlibrary".into(),
            status: "skipped".into(),
            description: None,
            rating: None,
            rating_count: None,
            url: None,
            fetched_at: now,
        };
    }
    let mut url = format!(
        "https://openlibrary.org/search.json?title={}&limit=3",
        urlencoding::encode(title)
    );
    if let Some(a) = author.map(str::trim).filter(|s| !s.is_empty()) {
        url.push_str(&format!("&author={}", urlencoding::encode(a)));
    }
    let v: Value = match agent().get(&url).call().and_then(|r| Ok(r.into_json()?)) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("openlibrary search failed: {e}");
            return error_entry("openlibrary", "network", now);
        }
    };
    let Some(doc) = v["docs"].as_array().and_then(|a| a.first()) else {
        return ExternalMetaEntry {
            source: "openlibrary".into(),
            status: "not_found".into(),
            description: None,
            rating: None,
            rating_count: None,
            url: None,
            fetched_at: now,
        };
    };
    let work_key = doc["key"].as_str().unwrap_or("");
    let mut description: Option<String> = None;
    let mut url_out: Option<String> = None;
    if !work_key.is_empty() {
        url_out = Some(format!("https://openlibrary.org{work_key}"));
        // Fetch the work page to get a description (search.json only gives
        // shallow metadata).
        let work_url = format!("https://openlibrary.org{work_key}.json");
        if let Ok(resp) = agent().get(&work_url).call() {
            if let Ok(w) = resp.into_json::<Value>() {
                description = match &w["description"] {
                    Value::String(s) => Some(clean_text(s)),
                    Value::Object(o) => o
                        .get("value")
                        .and_then(|v| v.as_str())
                        .map(clean_text),
                    _ => None,
                }
                .filter(|s| !s.is_empty());
            }
        }
    }
    let rating = doc["ratings_average"].as_f64();
    let rating_count = doc["ratings_count"].as_i64();
    ExternalMetaEntry {
        source: "openlibrary".into(),
        status: "ok".into(),
        description,
        rating,
        rating_count,
        url: url_out,
        fetched_at: now,
    }
}

fn error_entry(source: &str, status: &str, now: i64) -> ExternalMetaEntry {
    ExternalMetaEntry {
        source: source.to_string(),
        status: status.to_string(),
        description: None,
        rating: None,
        rating_count: None,
        url: None,
        fetched_at: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_strips_tags() {
        assert_eq!(
            clean_text("Hello <b>world</b>!\n\nNew line"),
            "Hello world! New line"
        );
    }
}
