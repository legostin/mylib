type Props = {
  label?: string;
  hint?: string | null;
};

/// Full-column overlay loader. Rendered above the .center column so the user
/// gets clear feedback that a query is running, while still being free to
/// keep typing in the search box or tweak filters (those live in the topbar,
/// which sits outside .center and stays interactive).
export function CenterLoader({ label = "Загрузка…", hint }: Props) {
  return (
    <div className="center-loader" role="status" aria-live="polite">
      <div className="center-loader-spinner" aria-hidden="true">
        <span />
        <span />
        <span />
      </div>
      <div className="center-loader-label">{label}</div>
      {hint && <div className="center-loader-hint">{hint}</div>}
    </div>
  );
}
