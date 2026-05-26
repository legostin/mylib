# MyLib

Локальный каталогизатор и читалка для библиотек в формате **INPX**.
Tauri 2 (Rust) + React + TypeScript + Vite.

## Что умеет

- Импортирует `.inpx` в локальную SQLite-базу с FTS5 (полнотекстовый поиск)
- **Поиск** по авторам, сериям и книгам — с подсветкой количества совпадений
- **Фильтры** под кнопкой: язык, жанр (на русском), автор с автодополнением, папка/архив. Активные фильтры висят чипсами под строкой поиска
- **Алфавитный указатель** авторов и серий с двухбуквенными префиксами (А → Аб → авторы). Кириллица всегда сверху, заглавные/строчные/ведущие скобки игнорируются
- **Каталог жанров** с переводом FB2-кодов на русский и группировкой по категориям (Фантастика / Проза / Детективы …)
- **Каталог языков** с человеческими названиями
- **Карточка автора**: hero c количеством книг/серий, табы «Серии / Все книги / Без серий», экспорт всего, добавление в списки
- **FB2 / EPUB ридер** в отдельном окне: оглавление (TOC), темы (светлая / сепия / тёмная), выбор шрифта (serif / sans / mono), масштаб, восстановление позиции чтения, навигация стрелками/PageUp/Down
- **Пользовательские списки** (Избранное, К прочтению, кастомные): мультивыбор книг/серий/авторов, bulk-добавление, удаление
- **Внешние метаданные**: подгружаются в фоне с Google Books и OpenLibrary, кешируются по `lib_id` на 30 дней
- **Экспорт книг** из спутниковых `.zip` в выбранную папку с прогрессом
- **OPDS-шаринг** локального каталога (опционально через ngrok) — открывается любым OPDS-ридером
- **SWR-кеш** на фронте: счётчики жанров/языков/букв обновляются в фоне, страница не моргает

## Требования

- Rust ≥ 1.77 (`rustup default stable`)
- Node ≥ 20 и pnpm (`brew install pnpm` или `corepack enable`)
- macOS: Xcode Command Line Tools (`xcode-select --install`)
- Linux: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libsoup-3.0-dev`, `librsvg2-dev` (и аналоги)
- Windows: MSVC Build Tools, WebView2 Runtime (есть в Windows 11; на 10 — устанавливается отдельно)

## Запуск в дев-режиме

```sh
pnpm install
pnpm tauri dev
```

Первая сборка Rust-зависимостей займёт несколько минут.

## Сборка установщика

```sh
pnpm tauri build
```

Артефакты появятся в `src-tauri/target/release/bundle/`:

- macOS: `.app` и `.dmg`
- Windows: `.msi` и `.exe` (NSIS)
- Linux: `.deb` и `.AppImage`

Готовые инсталляторы для каждой платформы автоматически собирает GitHub Actions на пуш тега `v*` — забирать из [Releases](https://github.com/legostin/mylib/releases).

## Где хранятся данные

База `library.db` + кеши лежат в системной папке пользовательских данных приложения (идентификатор `org.legostin.mylib`):

| Платформа | Путь                                                          |
|-----------|---------------------------------------------------------------|
| Windows   | `%APPDATA%\org.legostin.mylib\library.db`                     |
| Linux     | `~/.config/org.legostin.mylib/library.db`                     |
| macOS     | `~/Library/Application Support/org.legostin.mylib/library.db` |

В этой же папке хранятся:

- `library.db` — индекс + пользовательские списки + позиции чтения + кеш внешних метаданных
- `library.db-wal`, `library.db-shm` — служебные WAL-файлы SQLite

Чтобы сбросить всё (например, чтобы перенести библиотеку с нуля) — удалите эту папку перед запуском.

Пути к спутниковым `.zip` хранятся внутри `library.db` (поля `inpx_path`, `books_dir`); сам приложение их не копирует.

## Структура

```
mylib/
├── src/                       # React-фронтенд
│   ├── App.tsx                # Layout + роутинг по разделам
│   ├── components/            # Sidebar / BookList / BookDetail / Reader / …
│   └── lib/                   # API-обёртка, типы, кеш, словарь жанров
├── src-tauri/                 # Rust-бэкенд
│   ├── src/
│   │   ├── lib.rs             # Tauri builder + регистрация команд
│   │   ├── commands.rs        # #[tauri::command] точки
│   │   ├── library.rs         # импорт INPX → индекс, прогресс-события
│   │   ├── inpx.rs            # парсер .inpx (zip + .inp табл-записи)
│   │   ├── index.rs           # SQLite-индекс с FTS5, UDF для алфавита
│   │   ├── reader.rs          # FB2/EPUB → ReaderBook (главы, TOC, обложка)
│   │   ├── fb2.rs             # FB2 preview (annotation + cover)
│   │   ├── fb2_epub.rs        # Конвертация FB2 → EPUB
│   │   ├── external_meta.rs   # Google Books + OpenLibrary
│   │   ├── opds.rs            # OPDS-фид (atom XML)
│   │   ├── share.rs           # ngrok-туннель для шаринга
│   │   ├── export.rs          # извлечение книг из спутниковых zip
│   │   ├── model.rs           # модели данных
│   │   └── error.rs           # ошибки + serde
│   ├── capabilities/          # пермишены Tauri
│   └── tauri.conf.json
└── .github/workflows/         # CI + кросс-платформенный release
```

## Формат INPX (кратко)

- `.inpx` — ZIP-архив с файлами `*.inp`, `collection.info`, `version.info`, опционально `structure.info`
- Каждая `*.inp`-запись — одна строка, поля разделены байтом `0x04` (исторически — иногда `\t`)
- Поля по умолчанию: `AUTHOR;GENRE;TITLE;SERIES;SERNO;FILE;SIZE;LIBID;DEL;EXT;DATE;LANG;LIBRATE;KEYWORDS`
- `AUTHOR` — список через `:`, каждый автор `Last,First,Middle`
- `GENRE` — список через `:`
- Имя соседнего `.zip` с книгами берётся из имени `.inp` (`fb.foo.001-100.inp` → `fb.foo.001-100.zip`)
