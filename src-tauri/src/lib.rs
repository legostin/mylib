pub mod commands;
pub mod error;
pub mod export;
pub mod external_meta;
pub mod fb2;
pub mod fb2_epub;
pub mod index;
pub mod inpx;
pub mod library;
pub mod model;
pub mod opds;
pub mod reader;
pub mod share;

use std::sync::Arc;

use tauri::Manager;

use crate::library::{default_db_path, LibraryState};
use crate::share::ShareState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle();
            let db = default_db_path(handle)?;
            let state = LibraryState::open(db)?;
            app.manage(Arc::new(state));
            app.manage(Arc::new(ShareState::new()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_stats,
            commands::get_collection_info,
            commands::list_books,
            commands::get_book,
            commands::import_inpx,
            commands::export_books,
            commands::search,
            commands::get_author_view,
            commands::get_series_view,
            commands::get_book_content,
            commands::get_reader_book,
            commands::save_reading_position,
            commands::get_reading_position,
            commands::get_book_external_meta,
            commands::list_languages,
            commands::list_genres,
            commands::list_archives,
            commands::list_author_letters,
            commands::list_author_prefixes,
            commands::list_authors_by_letter,
            commands::list_series_letters,
            commands::list_series_by_letter,
            commands::list_lists,
            commands::create_list,
            commands::rename_list,
            commands::delete_list,
            commands::add_to_list,
            commands::remove_from_list,
            commands::lists_containing,
            commands::get_list_contents,
            commands::share_start,
            commands::share_stop,
            commands::share_status,
            commands::share_kill_stray,
            commands::share_list_domains,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
