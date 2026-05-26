/// Russian display names + categories for the standard FB2 genre dictionary.
///
/// FB2 codes follow the convention `<category>_<subgenre>`. We map the
/// commonly-seen codes from the FictionBook 2.x spec and the broader
/// community-extended set. Unknown codes fall back to the raw code with a
/// `?` marker; `categoryFor` slots them under "Другое".

export type GenreInfo = {
  ru: string;
  category: string;
};

/// Russian labels for the FB2 category roots — what the picker uses as group
/// headings on the Genres page.
export const CATEGORIES_RU: Record<string, string> = {
  sf: "Фантастика",
  prose: "Проза",
  detective: "Детективы",
  thriller: "Триллеры",
  love: "Любовные романы",
  adv: "Приключения",
  child: "Детская литература",
  poetry: "Поэзия и драма",
  antique: "Старинная литература",
  science: "Наука и образование",
  comp: "Компьютеры и интернет",
  reference: "Справочная литература",
  nonfiction: "Документальная литература",
  religion: "Религия и духовность",
  humor: "Юмор",
  home: "Дом и семья",
  business: "Деловая литература",
  other: "Другое",
};

const G: Record<string, GenreInfo> = {
  // ----- Sci-fi / fantasy --------------------------------------------------
  sf: { ru: "Научная фантастика", category: "sf" },
  sf_history: { ru: "Альтернативная история", category: "sf" },
  sf_action: { ru: "Боевая фантастика", category: "sf" },
  sf_epic: { ru: "Эпическая фантастика", category: "sf" },
  sf_heroic: { ru: "Героическая фантастика", category: "sf" },
  sf_detective: { ru: "Детективная фантастика", category: "sf" },
  sf_cyberpunk: { ru: "Киберпанк", category: "sf" },
  sf_space: { ru: "Космическая фантастика", category: "sf" },
  sf_social: { ru: "Социальная фантастика", category: "sf" },
  sf_horror: { ru: "Ужасы и мистика", category: "sf" },
  sf_humor: { ru: "Юмористическая фантастика", category: "sf" },
  sf_fantasy: { ru: "Фэнтези", category: "sf" },
  sf_fantasy_city: { ru: "Городское фэнтези", category: "sf" },
  sf_mystic: { ru: "Мистика", category: "sf" },
  sf_etc: { ru: "Прочая фантастика", category: "sf" },
  sf_postapocalyptic: { ru: "Постапокалипсис", category: "sf" },
  // (Philosophy lives in `science` further down; SF code reused.)

  // ----- Prose -------------------------------------------------------------
  prose_classic: { ru: "Классическая проза", category: "prose" },
  prose_history: { ru: "Историческая проза", category: "prose" },
  prose_contemporary: { ru: "Современная проза", category: "prose" },
  prose_counter: { ru: "Контркультура", category: "prose" },
  prose_rus_classic: { ru: "Русская классическая проза", category: "prose" },
  prose_su_classics: { ru: "Советская классика", category: "prose" },
  prose_military: { ru: "Военная проза", category: "prose" },
  short_story: { ru: "Малая проза", category: "prose" },
  prose: { ru: "Проза", category: "prose" },

  // ----- Detective / thriller ---------------------------------------------
  det_classic: { ru: "Классический детектив", category: "detective" },
  det_police: { ru: "Полицейский детектив", category: "detective" },
  det_action: { ru: "Боевик", category: "detective" },
  det_irony: { ru: "Иронический детектив", category: "detective" },
  det_history: { ru: "Исторический детектив", category: "detective" },
  det_espionage: { ru: "Шпионский детектив", category: "detective" },
  det_crime: { ru: "Криминальный детектив", category: "detective" },
  det_political: { ru: "Политический детектив", category: "detective" },
  det_maniac: { ru: "Маньяки", category: "detective" },
  det_hard: { ru: "Крутой детектив", category: "detective" },
  detective: { ru: "Детектив", category: "detective" },
  thriller: { ru: "Триллер", category: "thriller" },
  thriller_legal: { ru: "Юридический триллер", category: "thriller" },
  thriller_medical: { ru: "Медицинский триллер", category: "thriller" },
  thriller_techno: { ru: "Технотриллер", category: "thriller" },

  // ----- Love --------------------------------------------------------------
  love_contemporary: { ru: "Современные любовные романы", category: "love" },
  love_history: { ru: "Исторические любовные романы", category: "love" },
  love_detective: { ru: "Любовно-детективные романы", category: "love" },
  love_short: { ru: "Короткие любовные романы", category: "love" },
  love_erotica: { ru: "Эротика", category: "love" },
  love_sf: { ru: "Любовное фэнтези", category: "love" },
  love: { ru: "Любовный роман", category: "love" },

  // ----- Adventure --------------------------------------------------------
  adv_western: { ru: "Вестерн", category: "adv" },
  adv_history: { ru: "Исторические приключения", category: "adv" },
  adv_indian: { ru: "Приключения про индейцев", category: "adv" },
  adv_maritime: { ru: "Морские приключения", category: "adv" },
  adv_geo: { ru: "Путешествия и география", category: "adv" },
  adv_animal: { ru: "Природа и животные", category: "adv" },
  adventure: { ru: "Приключения", category: "adv" },

  // ----- Children ---------------------------------------------------------
  child_tale: { ru: "Сказка", category: "child" },
  child_verse: { ru: "Детские стихи", category: "child" },
  child_prose: { ru: "Детская проза", category: "child" },
  child_sf: { ru: "Детская фантастика", category: "child" },
  child_det: { ru: "Детский детектив", category: "child" },
  child_adv: { ru: "Детские приключения", category: "child" },
  child_education: { ru: "Детская образовательная", category: "child" },
  children: { ru: "Детское", category: "child" },
  child_classical: { ru: "Классическая детская литература", category: "child" },

  // ----- Poetry / drama ---------------------------------------------------
  poetry: { ru: "Поэзия", category: "poetry" },
  dramaturgy: { ru: "Драматургия", category: "poetry" },

  // ----- Antique / classics -----------------------------------------------
  antique_ant: { ru: "Античная литература", category: "antique" },
  antique_european: { ru: "Европейская старинная литература", category: "antique" },
  antique_russian: { ru: "Древнерусская литература", category: "antique" },
  antique_east: { ru: "Древневосточная литература", category: "antique" },
  antique_myths: { ru: "Мифы. Легенды. Эпос", category: "antique" },
  antique: { ru: "Старинная литература", category: "antique" },

  // ----- Science / education ----------------------------------------------
  sci_history: { ru: "История", category: "science" },
  sci_philology: { ru: "Языкознание", category: "science" },
  sci_culture: { ru: "Культурология", category: "science" },
  sci_religion: { ru: "Религиоведение", category: "science" },
  sci_psychology: { ru: "Психология", category: "science" },
  sci_philosophy: { ru: "Философия", category: "science" },
  sci_politics: { ru: "Политика", category: "science" },
  sci_business: { ru: "Деловая литература", category: "science" },
  sci_juris: { ru: "Юриспруденция", category: "science" },
  sci_linguistic: { ru: "Языкознание", category: "science" },
  sci_medicine: { ru: "Медицина", category: "science" },
  sci_chem: { ru: "Химия", category: "science" },
  sci_phys: { ru: "Физика", category: "science" },
  sci_biology: { ru: "Биология", category: "science" },
  sci_tech: { ru: "Технические науки", category: "science" },
  sci_math: { ru: "Математика", category: "science" },
  sci_cosmos: { ru: "Астрономия и Космос", category: "science" },
  sci_economy: { ru: "Экономика", category: "science" },
  sci_pedagogy: { ru: "Педагогика", category: "science" },
  sci_state: { ru: "Государство и право", category: "science" },
  sci_radio: { ru: "Радиоэлектроника", category: "science" },
  sci_geo: { ru: "Геология и география", category: "science" },
  sci_textbook: { ru: "Учебники", category: "science" },
  science: { ru: "Наука и образование", category: "science" },

  // ----- Computers / internet ---------------------------------------------
  computers: { ru: "Компьютеры", category: "comp" },
  comp_www: { ru: "Интернет", category: "comp" },
  comp_programming: { ru: "Программирование", category: "comp" },
  comp_hard: { ru: "Аппаратное обеспечение", category: "comp" },
  comp_soft: { ru: "Программы", category: "comp" },
  comp_db: { ru: "Базы данных", category: "comp" },
  comp_osnet: { ru: "ОС и сети", category: "comp" },
  comp_dsp: { ru: "Цифровая обработка сигналов", category: "comp" },
  comp_design: { ru: "Графика и дизайн", category: "comp" },

  // ----- Reference --------------------------------------------------------
  ref_encyc: { ru: "Энциклопедии", category: "reference" },
  ref_dict: { ru: "Словари", category: "reference" },
  ref_guide: { ru: "Справочники", category: "reference" },
  ref_ref: { ru: "Справочная литература", category: "reference" },
  geo_guides: { ru: "Путеводители", category: "reference" },

  // ----- Documentary / nonfiction -----------------------------------------
  nonf_biography: { ru: "Биографии и мемуары", category: "nonfiction" },
  nonf_publicism: { ru: "Публицистика", category: "nonfiction" },
  nonf_criticism: { ru: "Критика", category: "nonfiction" },
  design: { ru: "Искусство и дизайн", category: "nonfiction" },
  nonfiction: { ru: "Документальная литература", category: "nonfiction" },

  // ----- Religion ---------------------------------------------------------
  religion_rel: { ru: "Религия", category: "religion" },
  religion_esoterics: { ru: "Эзотерика", category: "religion" },
  religion_self: { ru: "Самосовершенствование", category: "religion" },
  religion: { ru: "Религия", category: "religion" },

  // ----- Humor ------------------------------------------------------------
  humor_anecdote: { ru: "Анекдоты", category: "humor" },
  humor_prose: { ru: "Юмористическая проза", category: "humor" },
  humor_verse: { ru: "Юмористические стихи", category: "humor" },
  humor: { ru: "Юмор", category: "humor" },

  // ----- Home / hobby -----------------------------------------------------
  home_cooking: { ru: "Кулинария", category: "home" },
  home_pets: { ru: "Домашние животные", category: "home" },
  home_crafts: { ru: "Хобби и ремёсла", category: "home" },
  home_entertain: { ru: "Развлечения", category: "home" },
  home_health: { ru: "Здоровье", category: "home" },
  home_garden: { ru: "Сад и огород", category: "home" },
  home_diy: { ru: "Сделай сам", category: "home" },
  home_sport: { ru: "Спорт", category: "home" },
  home_sex: { ru: "Эротика, секс", category: "home" },
  home_collecting: { ru: "Коллекционирование", category: "home" },
  home: { ru: "Дом и семья", category: "home" },

  // ----- Business --------------------------------------------------------
  banking: { ru: "Банковское дело", category: "business" },
  accounting: { ru: "Бухучёт", category: "business" },
  global_economy: { ru: "Мировая экономика", category: "business" },
  industries: { ru: "Отраслевые издания", category: "business" },
  job_hunting: { ru: "Поиск работы", category: "business" },
  management: { ru: "Менеджмент", category: "business" },
  marketing: { ru: "Маркетинг и PR", category: "business" },
  org_behavior: { ru: "Корпоративная культура", category: "business" },
  personal_finance: { ru: "Личные финансы", category: "business" },
  real_estate: { ru: "Недвижимость", category: "business" },
  small_business: { ru: "Малый бизнес", category: "business" },
  popular_business: { ru: "Популярная бизнес-литература", category: "business" },

  // ----- Misc -------------------------------------------------------------
  literature_18: { ru: "Зарубежная классика XVIII", category: "prose" },
  literature_19: { ru: "Зарубежная классика XIX", category: "prose" },
  literature_20: { ru: "Зарубежная классика XX", category: "prose" },
  notes: { ru: "Заметки", category: "other" },
  unrecognised: { ru: "Без жанра", category: "other" },
};

export function genreLabel(code: string): string {
  return G[code]?.ru ?? code;
}

export function categoryFor(code: string): string {
  const hit = G[code];
  if (hit) return hit.category;
  const dash = code.indexOf("_");
  const prefix = dash > 0 ? code.slice(0, dash) : code;
  return CATEGORIES_RU[prefix] ? prefix : "other";
}

export function categoryLabel(cat: string): string {
  return CATEGORIES_RU[cat] ?? "Другое";
}
