import type { CollectionInfo, LibraryStats } from "../lib/types";
import type { SidebarSection } from "./Sidebar";

type Props = {
  stats: LibraryStats | null;
  collection: CollectionInfo | null;
  onPickSection: (s: SidebarSection) => void;
};

/// Welcome dashboard rendered when the user lands on the "Библиотека" tab.
/// It avoids duplicating the alphabet (that's Авторы) and instead surfaces
/// counts + quick-jump cards into the four catalog views.
export function LibraryHome({ stats, collection, onPickSection }: Props) {
  const cards: {
    section: SidebarSection;
    title: string;
    sub: string;
    count?: number;
  }[] = [
    {
      section: "authors",
      title: "Авторы",
      sub: "Алфавитный указатель — буква, подбуква, имена",
      count: stats?.authors,
    },
    {
      section: "series",
      title: "Серии",
      sub: "Все циклы по первой букве",
      count: stats?.series,
    },
    {
      section: "genres",
      title: "Жанры",
      sub: "Подборки по тематике",
    },
    {
      section: "languages",
      title: "Языки",
      sub: "Каталог по языку оригинала",
    },
  ];

  return (
    <div className="entity-page library-home">
      <header className="entity-hero">
        <div className="section-label">Библиотека</div>
        <h1 className="entity-title">
          {collection?.name || "Каталог не загружен"}
        </h1>
        {collection?.version && (
          <div className="entity-stats">
            <span className="muted">v{collection.version}</span>
          </div>
        )}
        {stats && stats.books > 0 && (
          <div className="entity-stats">
            <span>
              <strong>{stats.books.toLocaleString("ru")}</strong> книг
            </span>
            <span className="dot">·</span>
            <span>
              <strong>{stats.authors.toLocaleString("ru")}</strong> авторов
            </span>
            <span className="dot">·</span>
            <span>
              <strong>{stats.series.toLocaleString("ru")}</strong> серий
            </span>
          </div>
        )}
      </header>

      <div className="home-cards">
        {cards.map((c) => (
          <button
            key={c.section}
            className="home-card"
            onClick={() => onPickSection(c.section)}
          >
            <div className="home-card-title">{c.title}</div>
            {c.count != null && (
              <div className="home-card-count">
                {c.count.toLocaleString("ru")}
              </div>
            )}
            <div className="home-card-sub">{c.sub}</div>
          </button>
        ))}
      </div>
    </div>
  );
}
