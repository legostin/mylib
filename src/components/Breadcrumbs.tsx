type Props = {
  items: { label: string; onClick?: () => void }[];
};

/// Renders a navigation trail. The last entry is the current page and is not
/// clickable; everything before it is rendered as a link the user can click
/// to pop back to that point in the history.
export function Breadcrumbs({ items }: Props) {
  if (items.length === 0) return null;
  return (
    <nav className="breadcrumbs" aria-label="Хлебные крошки">
      {items.map((it, i) => {
        const isLast = i === items.length - 1;
        return (
          <span key={i} className="breadcrumb-item">
            {isLast || !it.onClick ? (
              <span
                className={`breadcrumb-current${
                  isLast ? "" : " breadcrumb-static"
                }`}
                title={it.label}
              >
                {it.label}
              </span>
            ) : (
              <button
                className="breadcrumb-link"
                onClick={it.onClick}
                title={it.label}
              >
                {it.label}
              </button>
            )}
            {!isLast && <span className="breadcrumb-sep">›</span>}
          </span>
        );
      })}
    </nav>
  );
}
