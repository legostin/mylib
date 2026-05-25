import type { LanguageHit } from "../lib/types";

type Props = {
  languages: LanguageHit[];
  lang: string | null;
  onChange: (lang: string | null) => void;
  disabled?: boolean;
};

export function LanguagePicker({ languages, lang, onChange, disabled }: Props) {
  if (languages.length === 0) return null;
  return (
    <select
      className="lang-picker"
      value={lang ?? ""}
      onChange={(e) => onChange(e.target.value || null)}
      disabled={disabled}
      title="Язык книг"
    >
      <option value="">Все языки</option>
      {languages.map((l) => (
        <option key={l.code} value={l.code}>
          {l.code} · {l.count.toLocaleString("ru")}
        </option>
      ))}
    </select>
  );
}
