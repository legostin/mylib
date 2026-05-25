import type { BookFilters } from "../lib/types";
import { genreLabel } from "../lib/genresRu";

type Props = {
  filters: BookFilters;
  authorLabel?: string | null;
  onChange: (next: BookFilters) => void;
  onOpenPanel: () => void;
};

const LANG_NAMES: Record<string, string> = {
  ru: "Русский",
  en: "Английский",
  uk: "Украинский",
  be: "Белорусский",
  de: "Немецкий",
  fr: "Французский",
  es: "Испанский",
  it: "Итальянский",
};

function langName(code: string): string {
  return LANG_NAMES[code.toLowerCase()] ?? code;
}

/// Always-visible row of active filter chips. Each chip shows what's set
/// and lets the user either remove it (×) or jump into the filter panel to
/// edit (click on the label area).
export function ActiveFilterChips({
  filters,
  authorLabel,
  onChange,
  onOpenPanel,
}: Props) {
  const chips: { key: string; label: string; value: string; clear: () => void }[] = [];
  if (filters.lang) {
    chips.push({
      key: "lang",
      label: "Язык",
      value: langName(filters.lang),
      clear: () => onChange({ ...filters, lang: null }),
    });
  }
  if (filters.genre) {
    chips.push({
      key: "genre",
      label: "Жанр",
      value: genreLabel(filters.genre),
      clear: () => onChange({ ...filters, genre: null }),
    });
  }
  if (filters.authorId != null) {
    chips.push({
      key: "author",
      label: "Автор",
      value: authorLabel ?? `id ${filters.authorId}`,
      clear: () => onChange({ ...filters, authorId: null }),
    });
  }
  if (filters.archive) {
    chips.push({
      key: "archive",
      label: "Папка",
      value: filters.archive,
      clear: () => onChange({ ...filters, archive: null }),
    });
  }
  if (chips.length === 0) return null;
  return (
    <div className="active-filter-chips">
      {chips.map((c) => (
        <span key={c.key} className="active-chip">
          <button
            className="active-chip-body"
            onClick={onOpenPanel}
            title="Изменить фильтр"
          >
            <span className="active-chip-key">{c.label}:</span>
            <span className="active-chip-value">{c.value}</span>
          </button>
          <button
            className="active-chip-clear"
            onClick={c.clear}
            title="Убрать фильтр"
            aria-label="Убрать"
          >
            ×
          </button>
        </span>
      ))}
      {chips.length > 1 && (
        <button
          className="active-chip-clear-all"
          onClick={() =>
            onChange({
              lang: null,
              genre: null,
              archive: null,
              authorId: null,
            })
          }
        >
          Сбросить всё
        </button>
      )}
    </div>
  );
}
