type Props = {
  onExport: () => void | Promise<void>;
  disabled?: boolean;
  count?: number;
  label?: string;
};

export function ExportButton({ onExport, disabled, count, label }: Props) {
  const text = label ?? "Экспорт";
  return (
    <button
      type="button"
      className="export-btn"
      onClick={() => void onExport()}
      disabled={disabled}
      title={
        count !== undefined
          ? `Экспортировать ${count} ${plural(count)}`
          : "Экспортировать"
      }
    >
      ⬇ {text}
      {count !== undefined && count > 1 && (
        <span className="export-count">{count}</span>
      )}
    </button>
  );
}

function plural(n: number): string {
  const mod10 = n % 10;
  const mod100 = n % 100;
  if (mod10 === 1 && mod100 !== 11) return "книгу";
  if (mod10 >= 2 && mod10 <= 4 && (mod100 < 12 || mod100 > 14)) return "книги";
  return "книг";
}
