# MyLib

Кроссплатформенное приложение для работы с библиотеками в формате **INPX** (каталоги Флибусты и совместимые).
Tauri 2 (Rust) + React + TypeScript + Vite.

## Что уже умеет (v0.1, в работе)

- Открыть `.inpx` и проиндексировать каталог в локальную SQLite-базу
- Прогресс импорта в реальном времени
- Поиск по названию / автору / серии (FTS5)
- Виртуализированный список книг, панель деталей
- Авторы, жанры, серии — нормализованы в реляционной схеме

## Следующие шаги

- FB2-ридер прямо в окне (XML → структурированный текст с картинками)
- Экспорт `.fb2` из спутникового `.zip` (рядом с INPX)
- Конвертация FB2 → EPUB своим кодом, без Calibre

## Требования

- Rust ≥ 1.77 (`rustup default stable`)
- Node ≥ 20 и pnpm (`brew install pnpm` или `corepack enable`)
- macOS: Xcode Command Line Tools (`xcode-select --install`)

## Запуск в дев-режиме

```sh
pnpm install
pnpm tauri dev
```

Первая сборка Rust-зависимостей займёт несколько минут.

## Структура

```
mylib/
├── src/                       # React-фронтенд
│   ├── App.tsx                # 3-панельный layout
│   ├── components/            # Sidebar / BookList / BookDetail
│   └── lib/                   # API-обёртка над Tauri invoke, TS-типы
└── src-tauri/                 # Rust-бэкенд
    ├── src/
    │   ├── lib.rs             # Tauri builder + command registration
    │   ├── commands.rs        # #[tauri::command] точки
    │   ├── library.rs         # импорт INPX → индекс, прогресс-события
    │   ├── inpx.rs            # парсер .inpx (zip + .inp табл-записи)
    │   ├── index.rs           # SQLite-индекс с FTS5
    │   ├── model.rs           # модели данных
    │   └── error.rs           # ошибки + serde
    ├── capabilities/          # пермишены для plugin-dialog
    └── tauri.conf.json
```

## Формат INPX (кратко)

- `.inpx` — это ZIP-архив с файлами `*.inp`, `collection.info`, `version.info`, опционально `structure.info`.
- Каждая `*.inp`-запись — одна строка, поля разделены байтом `0x04` (исторически — иногда `\t`).
- Поля по умолчанию: `AUTHOR;GENRE;TITLE;SERIES;SERNO;FILE;SIZE;LIBID;DEL;EXT;DATE;LANG;LIBRATE;KEYWORDS`.
- `AUTHOR` — список через `:`, каждый автор `Last,First,Middle`.
- `GENRE` — список через `:`.
- Имя соседнего `.zip` с книгами берётся из имени `.inp` (`fb.foo.001-100.inp` → `fb.foo.001-100.zip`).

Данные индекса хранятся в `~/Library/Application Support/org.legostin.mylib/library.db`.
