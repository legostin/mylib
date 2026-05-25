import type { SearchScope } from "../lib/types";

type Props = {
  scope: SearchScope;
  onChange: (scope: SearchScope) => void;
  disabled?: boolean;
};

const OPTIONS: { value: SearchScope; label: string }[] = [
  { value: "all", label: "Все" },
  { value: "authors", label: "Авторы" },
  { value: "series", label: "Серии" },
  { value: "books", label: "Книги" },
];

export function ScopeChips({ scope, onChange, disabled }: Props) {
  return (
    <div className="scope-chips" role="tablist">
      {OPTIONS.map((o) => (
        <button
          key={o.value}
          type="button"
          role="tab"
          aria-selected={scope === o.value}
          className={`chip${scope === o.value ? " active" : ""}`}
          onClick={() => onChange(o.value)}
          disabled={disabled}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}
