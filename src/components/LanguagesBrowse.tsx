import { api } from "../lib/api";
import type { BookFilters, LanguageHit } from "../lib/types";
import { useSWR } from "../lib/useSWR";

type Props = {
  filters: BookFilters;
  onPickLang: (code: string) => void;
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
  pl: "Польский",
  cs: "Чешский",
  ja: "Японский",
  zh: "Китайский",
  ko: "Корейский",
  pt: "Португальский",
  nl: "Нидерландский",
  sv: "Шведский",
  no: "Норвежский",
  da: "Датский",
  fi: "Финский",
  hu: "Венгерский",
  ro: "Румынский",
  bg: "Болгарский",
  el: "Греческий",
  he: "Иврит",
  ar: "Арабский",
  tr: "Турецкий",
  hi: "Хинди",
  ka: "Грузинский",
  sr: "Сербский",
  hr: "Хорватский",
  sk: "Словацкий",
  lv: "Латышский",
  lt: "Литовский",
  et: "Эстонский",
  fa: "Персидский",
  vi: "Вьетнамский",
  th: "Тайский",
  id: "Индонезийский",
};

function langLabel(code: string): string {
  return LANG_NAMES[code.toLowerCase()] ?? code;
}

export function LanguagesBrowse({ filters, onPickLang }: Props) {
  const { data, loading, stale, error } = useSWR<LanguageHit[]>(
    "list_languages",
    { filters },
    () => api.languages(filters),
  );
  const items = data ?? [];

  const total = items.reduce((n, l) => n + l.count, 0);

  return (
    <div className="entity-page languages-browse">
      <header className="entity-hero">
        <div className="section-label">Каталог</div>
        <h1 className="entity-title">Языки</h1>
        <div className="entity-stats">
          <span>
            <strong>{items.length}</strong> языков
          </span>
          <span className="dot">·</span>
          <span>
            <strong>{total.toLocaleString("ru")}</strong> книг
          </span>
        </div>
      </header>

      {loading && items.length === 0 && (
        <div className="muted small" style={{ padding: "0 18px" }}>
          Загружаю языки…
        </div>
      )}
      {error && <div className="content-error">{error}</div>}

      <ul className={`row-list${stale ? " browse-refreshing" : ""}`}>
        {items.map((l) => (
          <li
            key={l.code}
            className="row lang"
            onClick={() => onPickLang(l.code)}
          >
            <div className="row-body">
              <div className="row-title">{langLabel(l.code)}</div>
              <div className="row-sub muted">{l.code}</div>
            </div>
            <span className="row-meta">{l.count.toLocaleString("ru")}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
