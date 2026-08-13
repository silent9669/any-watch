import type Hls from "hls.js";
import type { MediaPlayerClass, Representation } from "dashjs";
import {
  ArrowLeft,
  AlertTriangle,
  Check,
  ChevronLeft,
  ChevronRight,
  Clock,
  Copy,
  Download,
  Eye,
  EyeOff,
  Film,
  House,
  Loader2,
  LogOut,
  Maximize2,
  Pause,
  PictureInPicture2,
  Play,
  Plus,
  RotateCcw,
  RotateCw,
  Search,
  Settings2,
  ShieldCheck,
  SkipBack,
  SkipForward,
  SlidersHorizontal,
  Star,
  Trash2,
  UserPlus,
  Users,
  X,
  Volume2,
  VolumeX,
} from "lucide-react";
import { AnimatePresence, LayoutGroup, motion, useReducedMotion } from "framer-motion";
import { useEffect, useMemo, useRef, useState } from "react";
import type { FormEvent, ReactNode, SyntheticEvent } from "react";
import { animeKey, api, favoriteToAnime } from "./api";
import { episodeLabel, episodeTitleDetail } from "./episode-label";
import type {
  Anime,
  AnimeDetails,
  AppError,
  CatalogAnime,
  CatalogFilters,
  DiscoveryCatalog,
  Episode,
  EpisodeDownloadState,
  Favorite,
  ManagedUser,
  PlayerContext,
  ProviderAvailability,
  Source,
  SessionUser,
  SkipTime,
  WatchHistory,
} from "./types";

const SOURCE_STORAGE_KEY = "any-watch:selected-source";
const THEME_STORAGE_KEY = "any-watch:theme";
const APP_SCALE_STORAGE_KEY = "any-watch:scale";
const APP_FONT_STORAGE_KEY = "any-watch:font";
const SKIP_INTRO_STORAGE_KEY = "any-watch:skip-intro";
const EPISODE_RANGE_SIZE = 50;
const LOGO_SRC = "/logo.png";
const fadeUpVariant = {
  hidden: { opacity: 0, y: 18 },
  show: { opacity: 1, y: 0 },
};

type Route = "home" | "my-list" | "continue" | "admin" | "search" | "detail" | "catalog" | "settings";
type AppTheme = "obsidian" | "oled" | "ember" | "crimson" | "system";
type AppScale = "compact" | "comfortable" | "large" | "tv";
type AppFont = "manrope" | "noto" | "system";
type QualityLevel = { index: number; label: string; id?: string };
type ShelfSort = "recent" | "title" | "provider";
type HomeFeatureSlide = {
  id: string;
  kind: "personalMatch";
  title: string;
  image: string;
  description: string;
  context: string;
  progress: number;
  catalog?: CatalogAnime;
};

function App() {
  const [session, setSession] = useState<SessionUser | null | undefined>(undefined);
  const [authError, setAuthError] = useState<string | null>(null);
  const [sources, setSources] = useState<Source[]>([]);
  const [selectedSource, setSelectedSource] = useState<Source | null>(null);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<Anime[]>([]);
  const [catalogResults, setCatalogResults] = useState<CatalogAnime[]>([]);
  const [providerResults, setProviderResults] = useState<Anime[]>([]);
  const [catalogSelection, setCatalogSelection] = useState<CatalogAnime | null>(null);
  const [availability, setAvailability] = useState<ProviderAvailability[]>([]);
  const [catalogSearchError, setCatalogSearchError] = useState<AppError | null>(null);
  const [languageGroup, setLanguageGroup] = useState<"english" | "vietnamese">("english");
  const [discovery, setDiscovery] = useState<DiscoveryCatalog | null>(null);
  const [searchSelection, setSearchSelection] = useState<Anime | null>(null);
  const [selectedAnime, setSelectedAnime] = useState<Anime | null>(null);
  const [episodes, setEpisodes] = useState<Episode[]>([]);
  const [continueWatching, setContinueWatching] = useState<WatchHistory[]>([]);
  const [myList, setMyList] = useState<Favorite[]>([]);
  const [player, setPlayer] = useState<PlayerContext | null>(null);
  const [episodeDownloads, setEpisodeDownloads] = useState<Record<string, EpisodeDownloadState>>({});
  const [bootstrapping, setBootstrapping] = useState(true);
  const [loading, setLoading] = useState(false);
  const [loadingEpisodes, setLoadingEpisodes] = useState(false);
  const [error, setError] = useState<AppError | null>(null);
  const [route, setRoute] = useState<Route>("home");
  const [routeStack, setRouteStack] = useState<Route[]>([]);
  const detailCacheRef = useRef<Record<string, Partial<Anime>>>({});
  const availabilityCacheRef = useRef(new Map<string, { expiresAt: number; items: ProviderAvailability[] }>());
  const catalogSearchCacheRef = useRef(new Map<string, { expiresAt: number; items: CatalogAnime[] }>());
  const catalogCooldownUntilRef = useRef(0);
  const availabilityGenerationRef = useRef(0);
  const catalogSearchGenerationRef = useRef(0);
  const [providerHealthPending, setProviderHealthPending] = useState<string | null>(null);
  const [theme, setTheme] = useState<AppTheme>(loadSavedTheme);
  const [appScale, setAppScale] = useState<AppScale>(loadSavedScale);
  const [appFont, setAppFont] = useState<AppFont>(loadSavedFont);
  const [autoSkip, setAutoSkip] = useState(loadSavedAutoSkip);

  useEffect(() => {
    void bootstrap();
  }, []);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    saveTheme(theme);
  }, [theme]);

  useEffect(() => {
    document.documentElement.dataset.scale = appScale;
    saveScale(appScale);
  }, [appScale]);

  useEffect(() => {
    document.documentElement.dataset.font = appFont;
    saveFont(appFont);
  }, [appFont]);

  useEffect(() => {
    saveAutoSkip(autoSkip);
  }, [autoSkip]);

  useEffect(() => {
    const userAgent = navigator.userAgent.toLowerCase();
    const root = document.documentElement;
    root.classList.toggle("platform-macos", userAgent.includes("mac"));
    root.classList.toggle("platform-windows", userAgent.includes("win"));
    root.classList.toggle("platform-linux", userAgent.includes("linux"));

    const tvUserAgent = /(smart-tv|smarttv|tizen|web0s|webos|netcast|hbbtv|googletv|android tv|aft[bmst]|crkey)/.test(userAgent);
    const updateTvPlatform = () => {
      const remoteViewport = window.matchMedia("(min-width: 80rem) and (min-height: 40rem) and (hover: none)").matches;
      root.classList.toggle("platform-tv", tvUserAgent || remoteViewport);
    };
    updateTvPlatform();
    window.addEventListener("resize", updateTvPlatform);
    return () => window.removeEventListener("resize", updateTvPlatform);
  }, []);

  useEffect(() => {
    const handleTvNavigation = (event: KeyboardEvent) => {
      if (player || !["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(event.key)) return;
      const root = document.documentElement;
      if (!root.classList.contains("platform-tv") && root.dataset.scale !== "tv") return;

      const active = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      if (active?.matches("input, textarea, select, [contenteditable='true']")) return;

      const focusable = Array.from(
        document.querySelectorAll<HTMLElement>(
          "button:not([disabled]), a[href], input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex='-1'])",
        ),
      ).filter((element) => {
        const bounds = element.getBoundingClientRect();
        const style = window.getComputedStyle(element);
        return bounds.width > 0 && bounds.height > 0 && style.visibility !== "hidden" && style.display !== "none";
      });
      if (!focusable.length) return;

      if (!active || !focusable.includes(active)) {
        event.preventDefault();
        const preferred = focusable.find((element) => element.matches(".app-navigation-items > button.active")) ?? focusable[0];
        preferred.focus({ preventScroll: true });
        preferred.scrollIntoView({ block: "nearest", inline: "nearest" });
        return;
      }

      const current = active.getBoundingClientRect();
      const currentX = current.left + current.width / 2;
      const currentY = current.top + current.height / 2;
      const vertical = event.key === "ArrowUp" || event.key === "ArrowDown";
      const forward = event.key === "ArrowRight" || event.key === "ArrowDown";
      let next: HTMLElement | null = null;
      let bestScore = Number.POSITIVE_INFINITY;

      for (const candidate of focusable) {
        if (candidate === active) continue;
        const bounds = candidate.getBoundingClientRect();
        const deltaX = bounds.left + bounds.width / 2 - currentX;
        const deltaY = bounds.top + bounds.height / 2 - currentY;
        const primary = vertical ? deltaY : deltaX;
        if ((forward && primary <= 4) || (!forward && primary >= -4)) continue;
        const secondary = vertical ? deltaX : deltaY;
        const score = Math.abs(primary) + Math.abs(secondary) * 2.4;
        if (score < bestScore) {
          next = candidate;
          bestScore = score;
        }
      }

      if (!next) return;
      event.preventDefault();
      next.focus({ preventScroll: true });
      next.scrollIntoView({ behavior: "smooth", block: "nearest", inline: "nearest" });
    };

    window.addEventListener("keydown", handleTvNavigation);
    return () => window.removeEventListener("keydown", handleTvNavigation);
  }, [player, route]);

  useEffect(() => {
    if (route !== "search") return;
    const cleanQuery = query.trim();
    setAvailability([]);
    setSearchSelection(null);
    if (cleanQuery.length < 2) {
      setCatalogResults([]);
      setProviderResults([]);
      setCatalogSelection(null);
      setCatalogSearchError(null);
      return;
    }

    const handle = window.setTimeout(() => void searchCatalog(cleanQuery), 320);

    return () => window.clearTimeout(handle);
  }, [query, route, languageGroup, selectedSource?.name]);

  useEffect(() => {
    if (!sources.length) return;
    const current = selectedSource;
    if (
      current?.languageGroup === languageGroup
      && current.status === "healthy"
      && current.capabilities.search
    ) return;
    const nextSource = firstSearchableSource(sources, languageGroup);
    if (nextSource) {
      selectSource(nextSource);
    } else {
      setSelectedSource(null);
    }
  }, [sources, languageGroup, selectedSource?.name]);

  useEffect(() => {
    if (route !== "search" || !catalogSelection) return;
    void loadAvailability(catalogSelection, languageGroup);
  }, [route, catalogSelection?.catalogId, languageGroup]);

  async function bootstrap() {
    try {
      const currentSession = await api.getSession();
      setSession(currentSession);
      if (!currentSession) return;
      const [sourceList, history, favorites] = await Promise.all([
        api.listSources(),
        api.getContinueWatching(200),
        api.getMyList(300),
      ]);
      const savedSourceName = loadSavedSourceName();
      const savedSource = sourceList.find((source) =>
        source.name === savedSourceName
        && source.languageGroup === languageGroup
        && source.status === "healthy"
        && source.capabilities.search
      );
      const nextSource = savedSource ?? firstSearchableSource(sourceList, languageGroup);

      setSources(sourceList);
      setSelectedSource(nextSource);
      if (nextSource) saveSourceName(nextSource.name);
      setContinueWatching(history);
      setMyList(favorites);
      void api.listProviderHealth().then((health) => {
        setSources(health);
        setSelectedSource((current) => {
          const selected = health.find((source) => source.name === current?.name && source.status === "healthy");
          return selected ?? firstSearchableSource(health, languageGroup) ?? null;
        });
      }).catch((err) => {
        const appError = toAppError(err, "provider-health");
        setSources((current) => current.map((source) => source.status === "unknown" ? {
          ...source,
          status: "unavailable",
          failureCode: appError.code,
        } : source));
        setSelectedSource(null);
        setError(appError);
      });
      void api.getDiscovery().then((catalog) => {
        setDiscovery(catalog);
      }).catch((err) => setError(toAppError(err, "catalog")));
    } catch (err) {
      const appError = toAppError(err, "bootstrap");
      setError(appError);
      setAuthError(appError.message);
    } finally {
      setBootstrapping(false);
    }
  }

  async function refreshShelfData() {
    const [history, favorites] = await Promise.all([
      api.getContinueWatching(200),
      api.getMyList(300),
    ]);
    setContinueWatching(history);
    setMyList(favorites);
  }

  async function signIn(username: string, password: string) {
    setAuthError(null);
    setBootstrapping(true);
    try {
      await api.login(username, password);
      await bootstrap();
    } catch (err) {
      setAuthError(toAppError(err, "login").message);
      setBootstrapping(false);
    }
  }

  async function signOut() {
    try {
      await api.logout();
    } finally {
      setSession(null);
      setRoute("home");
      setRouteStack([]);
      setSources([]);
      setContinueWatching([]);
      setMyList([]);
    }
  }

  function navigate(nextRoute: Route) {
    if (nextRoute === route) return;
    setRouteStack((stack) => [...stack, route]);
    setRoute(nextRoute);
    setError(null);
  }

  function goBack() {
    const currentRoute = route;
    setRouteStack((stack) => {
      const nextStack = [...stack];
      const previous = nextStack.pop();
      setRoute(previous ?? "home");
      setError(null);
      if (currentRoute === "detail") {
        setSelectedAnime(null);
        setEpisodes([]);
      }
      return nextStack;
    });
  }

  function selectSource(source: Source) {
    setSelectedSource(source);
    saveSourceName(source.name);
  }

  function openSearch() {
    if (route !== "search") navigate("search");
  }

  async function searchCatalog(nextQuery = query, sourceOverride?: Source | null) {
    const cleanQuery = nextQuery.trim();
    const generation = ++catalogSearchGenerationRef.current;
    const activeSource = sourceOverride === undefined ? selectedSource : sourceOverride;
    if (cleanQuery.length < 2) {
      setCatalogResults([]);
      setProviderResults([]);
      setCatalogSelection(null);
      setSearchSelection(null);
      setCatalogSearchError(null);
      return;
    }

    setLoading(true);
    setError(null);
    setCatalogSearchError(null);
    setAvailability([]);
    setSearchSelection(null);
    try {
      const [catalogOutcome, providerOutcome] = await Promise.all([
        loadCatalogSearchResults(cleanQuery),
        activeSource
          ? searchProviderResults(cleanQuery, activeSource)
              .then((items) => ({ ok: true as const, items }))
              .catch((err) => ({ ok: false as const, error: toAppError(err, "provider-search") }))
          : Promise.resolve({ ok: true as const, items: [] }),
      ]);
      if (generation !== catalogSearchGenerationRef.current) return;
      const directItems = providerOutcome.ok ? providerOutcome.items : [];
      setProviderResults(directItems);
      if (!providerOutcome.ok) {
        setError(providerOutcome.error);
        if (activeSource && providerOutcome.error.code === "PROVIDER_CAPTCHA") {
          const blocked = { ...activeSource, status: "unavailable", failureCode: providerOutcome.error.code };
          setSources((current) => current.map((source) => source.name === blocked.name ? blocked : source));
          setSelectedSource(blocked);
        }
      }
      if (catalogOutcome.ok) {
        const items = catalogOutcome.items;
        setCatalogResults(items);
        const linkedDirectItems = directItems.map((anime, index) => {
          const catalogMatch = exactCatalogMatch(items, anime)
            ?? (index === 0 ? exactCatalogTitleMatch(items, cleanQuery) : null);
          return catalogMatch ? catalogToAnime(catalogMatch, anime) : anime;
        });
        setProviderResults(linkedDirectItems);
        if (linkedDirectItems.length) {
          setCatalogSelection(null);
          setSearchSelection(linkedDirectItems[0]);
        } else {
          setCatalogSelection(null);
        }
      } else {
        setCatalogResults([]);
        setCatalogSelection(null);
        setCatalogSearchError(catalogOutcome.error);
        if (directItems.length) {
          setSearchSelection(directItems[0]);
        } else {
          setSearchSelection(null);
          setError(catalogOutcome.error);
        }
      }
    } finally {
      if (generation === catalogSearchGenerationRef.current) setLoading(false);
    }
  }

  async function loadCatalogSearchResults(queryText: string): Promise<
    { ok: true; items: CatalogAnime[] } | { ok: false; error: AppError }
  > {
    const cacheKey = queryText.toLowerCase();
    const cached = catalogSearchCacheRef.current.get(cacheKey);
    if (cached && cached.expiresAt > Date.now()) {
      return { ok: true, items: cached.items };
    }
    if (catalogCooldownUntilRef.current > Date.now()) {
      return {
        ok: false,
        error: {
          code: "CATALOG_UNAVAILABLE",
          message: "AniList is rate limited. Showing provider results when available.",
          operation: "catalog-search",
          retryable: true,
          correlationId: crypto.randomUUID(),
          technical: "AniList search is cooling down after a 429 response.",
        },
      };
    }
    try {
      const items = await api.searchCatalog(queryText);
      catalogSearchCacheRef.current.set(cacheKey, { expiresAt: Date.now() + 10 * 60_000, items });
      return { ok: true, items };
    } catch (err) {
      const appError = toAppError(err, "catalog-search");
      if (appError.code === "CATALOG_UNAVAILABLE" && `${appError.technical ?? appError.message}`.includes("429")) {
        catalogCooldownUntilRef.current = Date.now() + 90_000;
      }
      return { ok: false, error: appError };
    }
  }

  async function searchProviderResults(queryText: string, source: Source) {
    if (!source.capabilities.search || source.status !== "healthy") return [];
    const items = await api.searchSource(source.name, queryText);

    const seen = new Set<string>();
    return items
      .map((item) => ({
        ...item,
        language: item.language || source.language,
        isFavorite: myList.some((favorite) => favorite.animeId === animeKey(item.provider, item.id)),
      }))
      .filter((item) => {
        const key = animeKey(item.provider, item.id);
        if (seen.has(key)) return false;
        seen.add(key);
        return true;
      })
      .slice(0, 36);
  }

  function selectProviderResult(anime: Anime) {
    const catalogMatch = anime.catalogId
      ? catalogResults.find((item) => item.catalogId === anime.catalogId) ?? null
      : exactCatalogMatch(catalogResults, anime);
    setCatalogSelection(null);
    setAvailability([]);
    setSelectedSource(sources.find((source) => source.name === anime.provider) ?? null);
    setSearchSelection(catalogMatch ? catalogToAnime(catalogMatch, anime) : anime);
  }

  function selectProviderSource(source: Source) {
    if (source.status !== "healthy") return;
    selectSource(source);
    const direct = providerResults.find((anime) => anime.provider === source.name);
    if (direct) {
      selectProviderResult(direct);
      return;
    }
    const option = availability.find((item) => item.provider === source.name);
    if (option?.anime) {
      void selectCatalogProvider(option);
      return;
    }
    setCatalogSelection(null);
    setSearchSelection(null);
    setProviderResults([]);
    if (query.trim().length >= 2) void searchCatalog(query, source);
  }

  async function retryProviderHealth(source: Source) {
    setProviderHealthPending(source.name);
    setError(null);
    try {
      const updates = await api.retryProviderHealth(source.name);
      const update = updates.find((item) => item.name === source.name);
      if (!update) return;
      setSources((current) => current.map((item) => item.name === update.name ? update : item));
      if (update.status === "healthy") {
        selectProviderSource(update);
      }
    } catch (err) {
      const appError = toAppError(err, "provider-health");
      setSources((current) => current.map((item) => item.name === source.name ? {
        ...item,
        status: "unavailable",
        failureCode: appError.code,
      } : item));
      setError(appError);
    } finally {
      setProviderHealthPending(null);
    }
  }

  function selectSearchLanguage(group: "english" | "vietnamese") {
    setLanguageGroup(group);
    const nextSource = firstSearchableSource(sources, group);
    if (nextSource) {
      selectSource(nextSource);
    } else {
      setSelectedSource(null);
    }
    setProviderResults([]);
    setSearchSelection(null);
    setAvailability([]);
    if (query.trim().length >= 2) void searchCatalog(query, nextSource);
  }

  function selectCatalogResult(anime: CatalogAnime) {
    setCatalogSelection((current) => {
      if (current?.catalogId === anime.catalogId) {
        return current;
      }
      return anime;
    });
    setSearchSelection(null);
  }

  async function loadAvailability(catalog: CatalogAnime, group: "english" | "vietnamese") {
    const generation = ++availabilityGenerationRef.current;
    const cacheKey = `${catalog.catalogId}:${group}`;
    setAvailability([]);
    setSearchSelection(null);
    setSelectedSource(null);
    setLoading(true);
    setError(null);
    try {
      const cached = availabilityCacheRef.current.get(cacheKey);
      const options = cached && cached.expiresAt > Date.now()
        ? cached.items
        : await api.resolveAvailability(catalog.catalogId, catalog.title, group);
      if (generation !== availabilityGenerationRef.current) return;
      availabilityCacheRef.current.set(cacheKey, { expiresAt: Date.now() + 5 * 60_000, items: options });
      setAvailability(options);
      const playable = options.find((item) =>
        item.status === "available"
        && item.anime
        && sources.some((source) => source.name === item.provider && source.status === "healthy")
      )?.anime ?? null;
      setSearchSelection(playable ? catalogToAnime(catalog, playable) : null);
      setSelectedSource(playable ? sources.find((source) => source.name === playable.provider) ?? null : null);
    } catch (err) {
      if (generation !== availabilityGenerationRef.current) return;
      setAvailability([]);
      setSearchSelection(null);
      setError(toAppError(err, "availability"));
    } finally {
      if (generation === availabilityGenerationRef.current) setLoading(false);
    }
  }

  async function selectCatalogProvider(option: ProviderAvailability) {
    if (!catalogSelection || !option.anime) return;
    const source = sources.find((item) => item.name === option.provider);
    if (!source || source.status !== "healthy") return;
    setSelectedSource(source);
    setSearchSelection(catalogToAnime(catalogSelection, option.anime));
  }

  function openCatalogSearch(catalog: CatalogAnime) {
    setQuery(catalog.title);
    setCatalogSelection(catalog);
    setCatalogResults([catalog]);
    setAvailability([]);
    setSearchSelection(null);
    if (route !== "search") navigate("search");
  }

  async function enrichAnime(anime: Anime): Promise<Anime> {
    const key = animeKey(anime.provider, anime.id);
    const cached = detailCacheRef.current[key];
    if (cached) return mergeAnimeDetails(anime, cached);

    try {
      const details = await api.getAnimeDetails(anime.provider, anime.id, anime.title);
      const patch = detailPatch(details);
      detailCacheRef.current[key] = patch;
      if (Object.keys(patch).length) mergeAnimeEverywhere(key, patch);
      return mergeAnimeDetails(anime, patch);
    } catch {
      detailCacheRef.current[key] = {};
      return anime;
    }
  }

  function mergeAnimeEverywhere(key: string, patch: Partial<Anime>) {
    setResults((items) =>
      items.map((item) => (animeKey(item.provider, item.id) === key ? mergeAnimeDetails(item, patch) : item)),
    );
    setProviderResults((items) =>
      items.map((item) => (animeKey(item.provider, item.id) === key ? mergeAnimeDetails(item, patch) : item)),
    );
    setSearchSelection((anime) =>
      anime && animeKey(anime.provider, anime.id) === key ? mergeAnimeDetails(anime, patch) : anime,
    );
    setSelectedAnime((anime) =>
      anime && animeKey(anime.provider, anime.id) === key ? mergeAnimeDetails(anime, patch) : anime,
    );
  }

  async function openAnime(anime: Anime) {
    setSelectedAnime(anime);
    setEpisodes([]);
    setLoadingEpisodes(true);
    setError(null);
    if (route !== "detail") navigate("detail");
    const linkedAnime = await linkCatalogAnime(anime);
    if (linkedAnime !== anime) setSelectedAnime(linkedAnime);
    void enrichAnime(linkedAnime);
    try {
      setEpisodes(await api.getEpisodes(linkedAnime.provider, linkedAnime.id));
    } catch (err) {
      const appError = toAppError(err, "episodes");
      if (providerFailureMakesOffline(appError)) markProviderOffline(anime.provider, appError.code);
      setError(appError);
    } finally {
      setLoadingEpisodes(false);
    }
  }

  async function linkCatalogAnime(anime: Anime): Promise<Anime> {
    if (anime.catalogId) return anime;
    const outcome = await loadCatalogSearchResults(anime.title);
    if (!outcome.ok) return anime;
    const match = exactCatalogMatch(outcome.items, anime);
    return match ? catalogToAnime(match, anime) : anime;
  }

  async function openHistoryItem(item: WatchHistory) {
    await openAnime(historyToAnime(item, myList));
  }

  async function toggleMyList(anime: Anime) {
    const key = animeKey(anime.provider, anime.id);
    try {
      if (anime.isFavorite || myList.some((item) => item.animeId === key)) {
        await api.removeFromMyList(key);
        setMyList((items) => items.filter((item) => item.animeId !== key));
        markFavorite(key, false);
      } else {
        await api.addToMyList(anime);
        await refreshShelfData();
        markFavorite(key, true);
      }
    } catch (err) {
      setError(toAppError(err, "favorites"));
    }
  }

  async function removeFromMyList(anime: Anime) {
    const key = animeKey(anime.provider, anime.id);
    try {
      await api.removeFromMyList(key);
      setMyList((items) => items.filter((item) => item.animeId !== key));
      markFavorite(key, false);
    } catch (err) {
      setError(toAppError(err, "favorites"));
    }
  }

  async function removeHistoryItem(item: WatchHistory) {
    try {
      await api.removeContinueWatching(item.animeId);
      setContinueWatching((items) => items.filter((current) => current.animeId !== item.animeId));
    } catch (err) {
      setError(toAppError(err, "history"));
    }
  }

  function markFavorite(key: string, isFavorite: boolean) {
    setResults((items) =>
      items.map((item) =>
        animeKey(item.provider, item.id) === key ? { ...item, isFavorite } : item,
      ),
    );
    setProviderResults((items) =>
      items.map((item) =>
        animeKey(item.provider, item.id) === key ? { ...item, isFavorite } : item,
      ),
    );
    setSearchSelection((anime) =>
      anime && animeKey(anime.provider, anime.id) === key ? { ...anime, isFavorite } : anime,
    );
    setSelectedAnime((anime) =>
      anime && animeKey(anime.provider, anime.id) === key ? { ...anime, isFavorite } : anime,
    );
  }

  async function playEpisode(anime: Anime, episode: Episode, startTime = 0, episodeList = episodes) {
    setError(null);
    try {
      const playback = await api.preparePlayback(anime.provider, episode.id);
      setPlayer({ anime, episode, episodes: episodeList, playback, startTime });
    } catch (err) {
      const appError = toAppError(err, "playback");
      if (providerFailureMakesOffline(appError)) markProviderOffline(anime.provider, appError.code);
      setError(appError);
    }
  }

  function markProviderOffline(provider: string, failureCode: string) {
    setSources((current) => current.map((source) =>
      source.name === provider
        ? { ...source, status: "unavailable", failureCode }
        : source
    ));
    setSelectedSource((current) => current?.name === provider ? null : current);
    availabilityCacheRef.current.clear();
  }

  async function downloadEpisode(anime: Anime, episode: Episode) {
    const key = episodeDownloadKey(anime, episode);
    const current = episodeDownloads[key];
    if (current?.status === "preparing" || current?.status === "downloading") return;
    const downloadId = crypto.randomUUID();
    const metadata = {
      downloadId,
      provider: anime.provider,
      animeId: anime.id,
      animeTitle: anime.title,
      coverUrl: anime.coverUrl,
      episodeId: episode.id,
      episodeNumber: episode.number,
      episodeTitle: episode.title,
    };

    setEpisodeDownloads((items) => ({
      ...items,
      [key]: { ...metadata, status: "preparing", progress: 0, message: "Preparing download..." },
    }));

    try {
      const result = await api.downloadEpisode(
        {
          id: downloadId,
          provider: anime.provider,
          animeId: anime.id,
          episodeId: episode.id,
          animeTitle: anime.title,
          coverUrl: anime.coverUrl,
          episodeNumber: episode.number,
          episodeTitle: episode.title,
        },
        (event) => {
          const segmentProgress = event.completedSegments && event.totalSegments
            ? `${event.completedSegments} / ${event.totalSegments} segments`
            : event.totalBytes
              ? `${formatBytes(event.downloadedBytes)} / ${formatBytes(event.totalBytes)}`
              : `${formatBytes(event.downloadedBytes)} downloaded`;
          setEpisodeDownloads((items) => ({
            ...items,
            [key]: {
              ...metadata,
              status: event.event === "started" ? "preparing" : "downloading",
              progress: Math.max(0, Math.min(100, event.progress || 0)),
              message: event.event === "started" ? `Saving ${event.fileName || `Episode ${episode.number}`}...` : segmentProgress,
              fileName: event.fileName ?? items[key]?.fileName,
            },
          }));
        },
      );
      setEpisodeDownloads((items) => ({
        ...items,
        [key]: {
          ...metadata,
          downloadId: result.id,
          status: "complete",
          progress: 100,
          message: "Browser download started",
          fileName: result.fileName,
        },
      }));
    } catch (err) {
      const appError = toAppError(err, "download");
      setEpisodeDownloads((items) => ({
        ...items,
        [key]: { ...metadata, status: "error", progress: 0, message: appError.message },
      }));
      setError(appError);
    }
  }

  const savedAnime = useMemo(() => myList.map(favoriteToAnime), [myList]);
  const latestHistory = continueWatching[0] ?? null;
  const featuredAnime = latestHistory ? historyToAnime(latestHistory, myList) : savedAnime[0] ?? null;
  const heroImage =
    selectedAnime?.bannerUrl ||
    selectedAnime?.coverUrl ||
    searchSelection?.bannerUrl ||
    searchSelection?.coverUrl ||
    featuredAnime?.bannerUrl ||
    featuredAnime?.coverUrl;
  const selectedAnimeIsFavorite = selectedAnime
    ? selectedAnime.isFavorite || myList.some((item) => item.animeId === animeKey(selectedAnime.provider, selectedAnime.id))
    : false;
  const resumeHistory = selectedAnime ? findHistoryForAnime(selectedAnime, continueWatching) : undefined;

  if (bootstrapping) {
    return <BootSplash />;
  }

  if (!session) {
    return <LoginScreen error={authError ?? error?.message ?? null} onLogin={signIn} />;
  }

  return (
    <div className={`app-shell route-${route} edition-web`}>
      <div
        className="ambient-backdrop"
        style={heroImage ? { backgroundImage: `url(${heroImage})` } : undefined}
      />

      <AppNavigation
        route={route}
        onNavigate={navigate}
      />

      <main>
        {error && (
          <ErrorNotice
            error={error}
            onDismiss={() => setError(null)}
            onRetry={error.retryable ? () => void bootstrap() : undefined}
          />
        )}

        <LayoutGroup id="any-watch-navigation">
        <AnimatePresence mode="wait" initial={false}>
          {route === "home" && (
            <motion.div key="home" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
              <HomeDashboard
                query={query}
                loading={loading}
                onOpenSearch={openSearch}
                continueItems={continueWatching.slice(0, 8)}
                continueTotal={continueWatching.length}
                discovery={discovery}
                savedAnime={savedAnime.slice(0, 10)}
                onOpenCatalog={openCatalogSearch}
                onOpenAnime={(anime) => void openAnime(anime)}
                onShowCatalog={() => navigate("catalog")}
                onResumeHistory={(item) => void openHistoryItem(item)}
                onShowHistory={continueWatching.length ? () => navigate("continue") : undefined}
                onShowMyList={() => navigate("my-list")}
                onShowSettings={() => navigate("settings")}
                session={session}
                onShowAdmin={session.role === "admin" ? () => navigate("admin") : undefined}
                onSignOut={() => void signOut()}
                myList={myList}
                onToggleFavorite={toggleMyList}
                onRemoveHistory={(item) => void removeHistoryItem(item)}
              />
            </motion.div>
          )}

          {route === "continue" && (
            <HistoryPage
              key="continue"
              items={continueWatching}
              onOpen={(item) => void openHistoryItem(item)}
              onRemove={(item) => void removeHistoryItem(item)}
              onBack={goBack}
              onOpenSearch={openSearch}
              myList={myList}
              onToggleFavorite={(item) => toggleMyList(historyToAnime(item, myList))}
            />
          )}

          {route === "my-list" && (
            <MyListPage
              key="my-list"
              items={savedAnime}
              onOpen={(anime) => void openAnime(anime)}
              onRemove={(anime) => void removeFromMyList(anime)}
              onBack={goBack}
              onOpenSearch={openSearch}
            />
          )}

          {route === "admin" && session.role === "admin" && (
            <AdminPage key="admin" currentUser={session} onBack={goBack} />
          )}

          {route === "settings" && (
            <SettingsPage
              key="settings"
              theme={theme}
              appScale={appScale}
              appFont={appFont}
              autoSkip={autoSkip}
              onBack={goBack}
              onThemeChange={setTheme}
              onScaleChange={setAppScale}
              onFontChange={setAppFont}
              onAutoSkipChange={setAutoSkip}
            />
          )}

          {route === "catalog" && (
            <CatalogPage
              key="catalog"
              genres={discovery?.genres ?? []}
              onBack={goBack}
              onOpen={openCatalogSearch}
            />
          )}

          {route === "search" && (
            <SearchStage
              key="search"
              query={query}
              results={catalogResults}
              providerResults={providerResults}
              catalogError={catalogSearchError}
              loading={loading}
              sources={sources}
              languageGroup={languageGroup}
              availability={availability}
              selectedSource={selectedSource}
              selectedCatalog={catalogSelection}
              selectedAnime={searchSelection}
              onQueryChange={setQuery}
              onSearch={() => void searchCatalog()}
              onLanguageChange={selectSearchLanguage}
              onProviderSelect={(option) => void selectCatalogProvider(option)}
              onProviderSourceSelect={selectProviderSource}
              onProviderHealthRetry={(source) => void retryProviderHealth(source)}
              providerHealthPending={providerHealthPending}
              onSelectProviderResult={selectProviderResult}
              onSelectCatalog={selectCatalogResult}
              onOpenAnime={(anime) => void openAnime(anime)}
              onDownload={(anime) => void openAnime(anime)}
              onToggleMyList={(anime) => void toggleMyList(anime)}
              onBack={goBack}
              myList={myList}
            />
          )}

          {route === "detail" && selectedAnime && (
            <DetailPage
              key={animeKey(selectedAnime.provider, selectedAnime.id)}
              anime={selectedAnime}
              episodes={episodes}
              loading={loadingEpisodes}
              isFavorite={selectedAnimeIsFavorite}
              resumeHistory={resumeHistory}
              onBack={goBack}
              onToggleMyList={() => void toggleMyList(selectedAnime)}
              onPlay={(episode, startTime) => void playEpisode(selectedAnime, episode, startTime)}
              onDownload={(episode) => void downloadEpisode(selectedAnime, episode)}
              downloadStates={episodeDownloads}
            />
          )}
        </AnimatePresence>
        </LayoutGroup>
      </main>

      <AnimatePresence>
        {player && (
          <VideoPlayer
            key="video-player"
            context={player}
            autoSkip={autoSkip}
            onAutoSkipChange={setAutoSkip}
            onPlayEpisode={(episode) => playEpisode(player.anime, episode, 0, player.episodes)}
            onClose={() => {
              setPlayer(null);
              void refreshShelfData();
            }}
          />
        )}
      </AnimatePresence>
    </div>
  );
}

function AppNavigation({
  route,
  onNavigate,
}: {
  route: Route;
  onNavigate: (route: Route) => void;
}) {
  const items: Array<{ route: Route; label: string; icon: ReactNode; badge?: number }> = [
    { route: "home", label: "Home", icon: <House size={20} /> },
    { route: "search", label: "Search", icon: <Search size={20} /> },
    { route: "my-list", label: "My List", icon: <Star size={20} /> },
    { route: "settings", label: "Settings", icon: <Settings2 size={20} /> },
  ];

  return (
    <nav className="app-navigation" aria-label="Primary navigation">
      <button className="app-navigation-brand" onClick={() => onNavigate("home")} aria-label="any-watch home">
        <img src={LOGO_SRC} alt="" />
        <span>any-watch</span>
      </button>
      <div className="app-navigation-items">
        {items.map((item) => (
          <button
            key={item.route}
            className={route === item.route ? "active" : ""}
            aria-current={route === item.route ? "page" : undefined}
            onClick={() => onNavigate(item.route)}
          >
            {item.icon}
            <span>{item.label}</span>
            {item.badge ? <b>{item.badge}</b> : null}
          </button>
        ))}
      </div>
    </nav>
  );
}

function SettingsPage({
  theme,
  appScale,
  appFont,
  autoSkip,
  onBack,
  onThemeChange,
  onScaleChange,
  onFontChange,
  onAutoSkipChange,
}: {
  theme: AppTheme;
  appScale: AppScale;
  appFont: AppFont;
  autoSkip: boolean;
  onBack: () => void;
  onThemeChange: (theme: AppTheme) => void;
  onScaleChange: (scale: AppScale) => void;
  onFontChange: (font: AppFont) => void;
  onAutoSkipChange: (enabled: boolean) => void;
}) {
  const themes: Array<{ id: AppTheme; name: string; description: string }> = [
    { id: "obsidian", name: "Obsidian Cinema", description: "Warm black, restrained red, full artwork." },
    { id: "oled", name: "OLED Theatre", description: "Deeper surfaces for dark rooms and phones." },
    { id: "ember", name: "Ember Room", description: "Warm charcoal with a softer vermilion accent." },
    { id: "crimson", name: "Crimson Noir", description: "Wine-black surfaces with a richer theatrical red." },
    { id: "system", name: "Device Contrast", description: "Follows the device contrast preference." },
  ];
  const scales: Array<{ id: AppScale; name: string; description: string }> = [
    { id: "compact", name: "Compact", description: "More titles and controls on a 16-inch display." },
    { id: "comfortable", name: "Comfortable", description: "Balanced spacing for everyday viewing." },
    { id: "large", name: "Large", description: "Larger text and touch targets for shared screens." },
    { id: "tv", name: "TV / remote", description: "10-foot text, generous safe margins, and arrow-key focus navigation." },
  ];
  const fonts: Array<{ id: AppFont; name: string; description: string }> = [
    { id: "manrope", name: "Manrope", description: "Modern interface face with Vietnamese support." },
    { id: "noto", name: "Noto Sans", description: "Highly legible Vietnamese and multilingual text." },
    { id: "system", name: "System", description: "Uses the native font on macOS, iPhone, or browser." },
  ];

  return (
    <motion.section className="settings-page" initial={{ opacity: 0 }} animate={{ opacity: 1 }} exit={{ opacity: 0 }}>
      <header className="settings-header">
        <IconButton label="Back" onClick={onBack}><ArrowLeft size={21} /></IconButton>
        <div>
          <p>Appearance</p>
          <h1>Settings</h1>
          <span>Choose playback behavior, interface size, theme, and reading font for this device.</span>
        </div>
      </header>

      <div className="settings-edit-grid">
        <section className="settings-edit-card settings-theme-card">
          <div className="settings-section-heading">
            <div><h2>Theme</h2><p>The black icon and restrained red accent remain consistent.</p></div>
          </div>
          <div className="theme-options" role="radiogroup" aria-label="Application theme">
            {themes.map((option) => (
              <button key={option.id} role="radio" aria-checked={theme === option.id} aria-label={`${option.name}. ${option.description}`} title={option.description} className={theme === option.id ? "active" : ""} onClick={() => onThemeChange(option.id)}>
                <i className={`theme-swatch theme-swatch-${option.id}`} />
                <span><strong>{option.name}</strong><small>{option.description}</small></span>
                {theme === option.id ? <Check size={17} /> : null}
              </button>
            ))}
          </div>
        </section>

        <section className="settings-edit-card">
          <div className="settings-section-heading">
            <div><h2>Interface size</h2><p>Balance information density with comfortable touch targets.</p></div>
          </div>
          <div className="appearance-options" role="radiogroup" aria-label="Interface size">
            {scales.map((option) => (
              <button key={option.id} role="radio" aria-checked={appScale === option.id} className={appScale === option.id ? "active" : ""} onClick={() => onScaleChange(option.id)}>
                <span><strong>{option.name}</strong><small>{option.description}</small></span>
                {appScale === option.id ? <Check size={17} /> : null}
              </button>
            ))}
          </div>
        </section>

        <section className="settings-edit-card">
          <div className="settings-section-heading">
            <div><h2>Reading font</h2><p>All choices support Vietnamese titles and interface text.</p></div>
          </div>
          <div className="appearance-options" role="radiogroup" aria-label="Reading font">
            {fonts.map((option) => (
              <button key={option.id} role="radio" aria-checked={appFont === option.id} className={appFont === option.id ? "active" : ""} onClick={() => onFontChange(option.id)}>
                <span><strong>{option.name}</strong><small>{option.description}</small></span>
                {appFont === option.id ? <Check size={17} /> : null}
              </button>
            ))}
          </div>
        </section>

        <section className="settings-edit-card settings-playback-card">
          <div className="settings-section-heading">
            <div><h2>Playback</h2><p>Use AniSkip community timing data to skip known openings.</p></div>
          </div>
          <div className="appearance-options" role="radiogroup" aria-label="Skip intro">
            <button role="radio" aria-checked={autoSkip} className={autoSkip ? "active" : ""} onClick={() => onAutoSkipChange(true)}>
              <span><strong>Skip intro on</strong><small>Skip a verified opening when playback enters its marked range.</small></span>
              {autoSkip ? <Check size={17} /> : null}
            </button>
            <button role="radio" aria-checked={!autoSkip} className={!autoSkip ? "active" : ""} onClick={() => onAutoSkipChange(false)}>
              <span><strong>Skip intro off</strong><small>Play openings normally while keeping timing markers visible.</small></span>
              {!autoSkip ? <Check size={17} /> : null}
            </button>
          </div>
        </section>
      </div>
    </motion.section>
  );
}

function providerStatusLabel(source: Source) {
  if (source.status === "healthy") return "Online";
  if (source.status === "degraded") return "Limited";
  if (source.status === "unavailable") return "Offline";
  return "Checking";
}

function LoginScreen({
  error,
  onLogin,
}: {
  error: string | null;
  onLogin: (username: string, password: string) => Promise<void>;
}) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  return (
    <main className="login-screen">
      <div className="login-ambient" />
      <section className="login-showcase" aria-label="any-watch family theatre">
        <div className="login-showcase-brand">
          <img src={LOGO_SRC} alt="" />
          <span>any-watch</span>
        </div>
        <div className="login-showcase-copy">
          <p>Private family theatre</p>
          <h1>Pick a source.<br />Keep your place.</h1>
          <span>One watchlist for your family, with provider-specific search and episode progress on every signed-in device.</span>
        </div>
        <dl className="login-showcase-facts">
          <div><dt>Catalogs</dt><dd>Provider-first</dd></div>
          <div><dt>Access</dt><dd>Family accounts</dd></div>
          <div><dt>Playback</dt><dd>Any browser</dd></div>
        </dl>
      </section>
      <motion.section
        className="login-card"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.22 }}
      >
        <div className="login-brand">
          <img src={LOGO_SRC} alt="any-watch" />
          <div><span>any-watch</span><small>Signed-in family access</small></div>
        </div>
        <div className="login-copy">
          <p className="eyebrow">Private watch space</p>
          <h2>Sign in</h2>
          <p>Use the account created by your family administrator.</p>
        </div>
        <form
          onSubmit={(event) => {
            event.preventDefault();
            if (!username.trim() || !password || submitting) return;
            setSubmitting(true);
            void onLogin(username.trim(), password).finally(() => setSubmitting(false));
          }}
        >
          <label>
            <span>Username</span>
            <input autoComplete="username" autoCapitalize="none" autoCorrect="off" spellCheck={false} value={username} onChange={(event) => setUsername(event.target.value)} autoFocus />
          </label>
          <label className="login-password-label">
            <span>Password</span>
            <span className="login-password-field">
              <input
                type={showPassword ? "text" : "password"}
                autoComplete="current-password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
              />
              <button
                type="button"
                className="login-password-toggle"
                aria-label={showPassword ? "Hide password" : "Show password"}
                aria-pressed={showPassword}
                onClick={() => setShowPassword((visible) => !visible)}
              >
                {showPassword ? <EyeOff size={18} /> : <Eye size={18} />}
              </button>
            </span>
          </label>
          {error && (
            <div className="login-error" role="alert">
              <AlertTriangle size={16} />
              <span>{error}<small>Safari may reuse an older saved password. Reveal the field and verify it before retrying.</small></span>
            </div>
          )}
          <button className="primary" disabled={submitting || !username.trim() || !password}>
            {submitting ? <Loader2 className="spin" size={18} /> : <ChevronRight size={18} />}
            {submitting ? "Signing in…" : "Sign in"}
          </button>
        </form>
        <small className="login-footnote">Accounts are created by your any-watch administrator.</small>
      </motion.section>
    </main>
  );
}

function BootSplash() {
  return (
    <div className="boot-screen">
      <motion.img
        src={LOGO_SRC}
        alt="any-watch"
        initial={{ opacity: 0, scale: 0.9, rotate: -2 }}
        animate={{ opacity: 1, scale: [0.9, 1.03, 1], rotate: 0 }}
        transition={{ duration: 1.1, ease: "easeOut" }}
      />
      <motion.div
        className="boot-progress"
        initial={{ scaleX: 0 }}
        animate={{ scaleX: 1 }}
        transition={{ duration: 1.4, ease: "easeInOut", repeat: Infinity, repeatType: "reverse" }}
      />
    </div>
  );
}

function ErrorNotice({
  error,
  onDismiss,
  onRetry,
}: {
  error: AppError;
  onDismiss: () => void;
  onRetry?: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  return (
    <motion.aside
      className="error-notice"
      initial={{ opacity: 0, y: -12, scale: 0.98 }}
      animate={{ opacity: 1, y: 0, scale: 1 }}
      role="alert"
    >
      <AlertTriangle size={19} />
      <div className="error-notice-copy">
        <strong>{error.code}</strong>
        <span>{error.message}</span>
        {expanded && (
          <dl>
            <div><dt>Operation</dt><dd>{error.operation}</dd></div>
            {error.provider && <div><dt>Provider</dt><dd>{error.provider}</dd></div>}
            <div><dt>Correlation</dt><dd>{error.correlationId}</dd></div>
            {error.technical && <div><dt>Details</dt><dd>{error.technical}</dd></div>}
          </dl>
        )}
      </div>
      <div className="error-notice-actions">
        <button onClick={() => setExpanded((value) => !value)}>{expanded ? "Less" : "Details"}</button>
        <IconButton label="Copy error details" onClick={() => void navigator.clipboard?.writeText(JSON.stringify(error, null, 2))}>
          <Copy size={17} />
        </IconButton>
        {onRetry && <button className="primary" onClick={onRetry}>Retry</button>}
        <IconButton label="Dismiss error" onClick={onDismiss}><X size={17} /></IconButton>
      </div>
    </motion.aside>
  );
}

function HomeDashboard({
  query,
  loading,
  onOpenSearch,
  continueItems,
  continueTotal,
  discovery,
  savedAnime,
  onOpenCatalog,
  onOpenAnime,
  onShowCatalog,
  onResumeHistory,
  onShowHistory,
  onShowMyList,
  onShowSettings,
  session,
  onShowAdmin,
  onSignOut,
  myList,
  onToggleFavorite,
  onRemoveHistory,
}: {
  query: string;
  loading: boolean;
  onOpenSearch: () => void;
  continueItems: WatchHistory[];
  continueTotal: number;
  discovery: DiscoveryCatalog | null;
  savedAnime: Anime[];
  onOpenCatalog: (anime: CatalogAnime) => void;
  onOpenAnime: (anime: Anime) => void;
  onShowCatalog: () => void;
  onResumeHistory: (item: WatchHistory) => void;
  onShowHistory?: () => void;
  onShowMyList: () => void;
  onShowSettings: () => void;
  session: SessionUser;
  onShowAdmin?: () => void;
  onSignOut?: () => void;
  myList: Favorite[];
  onToggleFavorite: (anime: Anime) => void;
  onRemoveHistory: (item: WatchHistory) => void;
}) {
  const shouldReduceMotion = useReducedMotion();
  const [featureIndex, setFeatureIndex] = useState(0);
  const [featurePaused, setFeaturePaused] = useState(false);
  const [featureInteracting, setFeatureInteracting] = useState(false);
  const [documentVisible, setDocumentVisible] = useState(!document.hidden);
  const personalMatches = useMemo(() => {
    const candidates = [...(discovery?.trending ?? []), ...(discovery?.popularThisSeason ?? [])];
    const uniqueCandidates = [...new Map(candidates.map((item) => [item.catalogId, item])).values()];
    return sortCatalogByPersonalMatch(uniqueCandidates);
  }, [discovery?.trending, discovery?.popularThisSeason]);
  const featureSlides = useMemo<HomeFeatureSlide[]>(() => [
    ...personalMatches.slice(0, 10).map((item) => ({
      id: `personal-match:${item.catalogId}`,
      kind: "personalMatch" as const,
      title: item.title,
      image: item.bannerUrl || item.coverUrl || LOGO_SRC,
      description: plainDescription(item.description) || "Open the title, choose a provider, and see the episodes available to your family.",
      context: item.personalMatch != null ? `${item.personalMatch}% personal match` : "Recommended for you",
      progress: 0,
      catalog: item,
    })),
  ], [personalMatches]);
  const featured = featureSlides[featureIndex] ?? featureSlides[0] ?? {
    id: "any-watch",
    kind: "personalMatch" as const,
    title: "any-watch",
    image: LOGO_SRC,
    description: "Choose one provider catalog, find an episode, and settle in.",
    context: "Private family theatre",
    progress: 0,
  };
  const featuredTitleClass = featured.title.length > 72
    ? "very-long"
    : featured.title.length > 38
      ? "long"
      : undefined;

  useEffect(() => {
    setFeatureIndex((current) => Math.min(current, Math.max(0, featureSlides.length - 1)));
  }, [featureSlides.length]);

  useEffect(() => {
    const handleVisibility = () => setDocumentVisible(!document.hidden);
    document.addEventListener("visibilitychange", handleVisibility);
    return () => document.removeEventListener("visibilitychange", handleVisibility);
  }, []);

  useEffect(() => {
    if (shouldReduceMotion || featurePaused || featureInteracting || !documentVisible || featureSlides.length < 2) return undefined;
    const interval = window.setInterval(() => {
      setFeatureIndex((current) => (current + 1) % featureSlides.length);
    }, 4200);
    return () => window.clearInterval(interval);
  }, [shouldReduceMotion, featurePaused, featureInteracting, documentVisible, featureSlides.length]);

  function showPreviousFeature() {
    setFeatureIndex((current) => (current - 1 + featureSlides.length) % featureSlides.length);
  }

  function showNextFeature() {
    setFeatureIndex((current) => (current + 1) % featureSlides.length);
  }

  return (
    <section className="home-dashboard">
      <img className="home-stage-watermark" src={LOGO_SRC} alt="" aria-hidden="true" />
      <motion.div
        className="home-command-center"
        initial="hidden"
        animate="show"
        onMouseEnter={() => setFeatureInteracting(true)}
        onMouseLeave={() => setFeatureInteracting(false)}
        onFocusCapture={() => setFeatureInteracting(true)}
        onBlurCapture={(event) => {
          if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFeatureInteracting(false);
        }}
        variants={{
          hidden: { opacity: 0 },
          show: {
            opacity: 1,
            transition: shouldReduceMotion
              ? { duration: 0.15 }
              : { duration: 0.3, ease: "easeOut", staggerChildren: 0.055 },
          },
        }}
      >
        <AnimatePresence mode="wait" initial={false}>
          <motion.div
            key={featured.id}
            className="home-feature-art"
            style={{ backgroundImage: `url(${featured.image})` }}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: shouldReduceMotion ? 0.15 : 0.34, ease: [0.16, 1, 0.3, 1] }}
            aria-hidden="true"
          />
        </AnimatePresence>
        <div className="home-feature-veil" aria-hidden="true" />
        <AnimatePresence mode="wait" initial={false}>
          <motion.div
            key={`copy:${featured.id}`}
            className="home-feature-copy"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: shouldReduceMotion ? 0.15 : 0.22 }}
          >
            <p className="home-feature-context">{featured.context}</p>
            <h1 className={featuredTitleClass}>{featured.title}</h1>
            <p className="home-feature-description">{featured.description}</p>
            <div className="home-feature-actions">
              {featured.catalog ? (
                <button className="primary" onClick={() => onOpenCatalog(featured.catalog!)}>
                  <Play size={18} fill="currentColor" /> Watch now
                </button>
              ) : null}
            </div>
          </motion.div>
        </AnimatePresence>
        <motion.div className="home-command-actions" variants={shouldReduceMotion ? { hidden: { opacity: 0 }, show: { opacity: 1 } } : fadeUpVariant}>
          <button
            className="hero-search-trigger home-command-search"
            onClick={onOpenSearch}
          >
            <Search size={20} />
            <span>{query.trim() || "Search anime, films, OVAs..."}</span>
            {loading ? <Loader2 className="spin" size={18} /> : <ChevronRight size={19} />}
          </button>
          <p className="home-command-hint">Search stays attached to the provider you choose.</p>
          <div className="home-command-shortcuts">
            <button onClick={onShowMyList}><Star size={16} /> My List</button>
            <button onClick={onShowSettings}><Settings2 size={16} /> Settings</button>
            {onShowAdmin && <button onClick={onShowAdmin}><ShieldCheck size={16} /> Users</button>}
            {onSignOut && <button onClick={onSignOut}><LogOut size={16} /> Sign out {session.username}</button>}
          </div>
        </motion.div>
        {featureSlides.length > 1 ? (
          <div className="home-feature-controls" aria-label="Featured title controls">
            <button onClick={showPreviousFeature} aria-label="Previous featured title"><ChevronLeft size={17} /></button>
            {!shouldReduceMotion && (
              <button onClick={() => setFeaturePaused((paused) => !paused)} aria-label={featurePaused ? "Play featured titles" : "Pause featured titles"}>
                {featurePaused ? <Play size={16} /> : <Pause size={16} />}
              </button>
            )}
            <button onClick={showNextFeature} aria-label="Next featured title"><ChevronRight size={17} /></button>
            <div className="home-feature-dots" role="group" aria-label="Choose featured title">
              {featureSlides.map((slide, index) => (
                <button
                  key={slide.id}
                  className={featureIndex === index ? "active" : ""}
                  onClick={() => setFeatureIndex(index)}
                  aria-label={`Show ${slide.title}`}
                  aria-current={featureIndex === index ? "true" : undefined}
                />
              ))}
            </div>
          </div>
        ) : null}
      </motion.div>

      <div className="dashboard-shelves">
        <ContinueWatchingRow
          items={continueItems}
          total={continueTotal}
          onOpen={onResumeHistory}
          onShowMore={onShowHistory}
          myList={myList}
          onToggleFavorite={(item) => onToggleFavorite(historyToAnime(item, myList))}
          onRemove={onRemoveHistory}
        />
        <CatalogRow
          title="Top Matches"
          items={personalMatches}
          loading={!discovery}
          onOpen={onOpenCatalog}
          onShowMore={onShowCatalog}
        />
        <AnimeRow
          title="My List"
          items={savedAnime}
          onOpen={onOpenAnime}
          onShowMore={onShowMyList}
          myList={myList}
          onToggleFavorite={onToggleFavorite}
          onRemove={onToggleFavorite}
          emptyTitle="Your list is empty"
          emptySubtitle="Search and add titles to keep them here."
        />
      </div>
    </section>
  );
}

function CatalogRow({
  title,
  items,
  loading,
  onOpen,
  controls,
  onShowMore,
  emptyTitle,
  emptySubtitle,
}: {
  title: string;
  items: CatalogAnime[];
  loading?: boolean;
  onOpen: (anime: CatalogAnime) => void;
  controls?: ReactNode;
  onShowMore?: () => void;
  emptyTitle?: string;
  emptySubtitle?: string;
}) {
  return (
    <motion.section className="content-row catalog-row" initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }}>
      <div className="row-heading"><h2>{title}</h2>{controls}{onShowMore && <button onClick={onShowMore}>Show More <ChevronRight size={17} /></button>}</div>
      <div className={items.length || loading ? "card-row" : "card-row empty-row"}>
        {loading
          ? Array.from({ length: 9 }).map((_, index) => <div className="catalog-card skeleton" key={index} />)
          : items.length ? items.map((anime, index) => (
            <motion.button
              className="catalog-card"
              key={anime.catalogId}
              onClick={() => onOpen(anime)}
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ delay: Math.min(index * 0.025, 0.15) }}
            >
              <img src={anime.coverUrl || LOGO_SRC} alt="" onError={useLogoFallback} />
              <span>{anime.title}</span>
              <small>{anime.personalMatch != null ? `${anime.personalMatch}% match` : anime.score ? `${anime.score}% score` : anime.format || "Anime"}</small>
            </motion.button>
          )) : <ShelfEmptyCard title={emptyTitle || "Nothing here yet"} subtitle={emptySubtitle || "Try another catalog filter."} />}
      </div>
    </motion.section>
  );
}

function ProviderChips({
  sources,
  selected,
  onSelect,
}: {
  sources: Source[];
  selected: Source | null;
  onSelect: (source: Source) => void;
}) {
  if (!sources.length) return <p className="source-empty">No providers enabled.</p>;

  return (
    <div className="provider-strip" aria-label="Search providers">
      {sources.map((source) => (
        <button
          key={source.name}
          className={selected?.name === source.name ? "provider-chip active" : "provider-chip"}
          aria-pressed={selected?.name === source.name}
          onClick={() => onSelect(source)}
        >
          <strong>{source.name}</strong>
          <span>{source.language}</span>
        </button>
      ))}
    </div>
  );
}

function ContinueWatchingRow({
  items,
  total,
  onOpen,
  onShowMore,
  myList,
  onToggleFavorite,
  onRemove,
}: {
  items: WatchHistory[];
  total: number;
  onOpen: (item: WatchHistory) => void;
  onShowMore?: () => void;
  myList: Favorite[];
  onToggleFavorite: (item: WatchHistory) => void;
  onRemove: (item: WatchHistory) => void;
}) {
  return (
    <motion.section className="content-row" initial={{ opacity: 0, y: 14 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.24 }}>
      <RowHeading title="Continue Watching" total={total} onShowMore={onShowMore} />
      <div className={items.length ? "card-row" : "card-row empty-row"}>
        {items.length ? (
          items.map((item) => (
            <HistoryCard
              item={item}
              key={item.animeId}
              onOpen={onOpen}
              isFavorite={myList.some((fav) => fav.animeId === item.animeId)}
              onToggleFavorite={onToggleFavorite}
              onRemove={onRemove}
            />
          ))
        ) : (
          <ShelfEmptyCard title="Nothing to resume" subtitle="Start an episode and it will appear here." />
        )}
      </div>
    </motion.section>
  );
}

function AnimeRow({
  title,
  items,
  total,
  loading,
  onOpen,
  onShowMore,
  myList,
  onToggleFavorite,
  onRemove,
  emptyTitle = "Nothing here yet",
  emptySubtitle = "Search anime and add a title.",
}: {
  title: string;
  items: Anime[];
  total?: number;
  loading?: boolean;
  onOpen: (anime: Anime) => void;
  onShowMore?: () => void;
  myList: Favorite[];
  onToggleFavorite: (anime: Anime) => void;
  onRemove?: (anime: Anime) => void;
  emptyTitle?: string;
  emptySubtitle?: string;
}) {
  return (
    <motion.section className="content-row" initial={{ opacity: 0, y: 14 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.24, delay: 0.04 }}>
      <RowHeading title={title} total={total ?? items.length} onShowMore={onShowMore} />
      <div className={loading || items.length ? "card-row" : "card-row empty-row"}>
        {loading
          ? Array.from({ length: 8 }).map((_, index) => <div className="poster-card skeleton" key={index} />)
          : items.length
            ? items.map((anime) => (
              <AnimeCard
                anime={anime}
                key={`${anime.provider}:${anime.id}`}
                onOpen={onOpen}
                isFavorite={myList.some((fav) => fav.animeId === animeKey(anime.provider, anime.id))}
                onToggleFavorite={onToggleFavorite}
                onRemove={onRemove}
              />
            ))
            : <ShelfEmptyCard title={emptyTitle} subtitle={emptySubtitle} />}
      </div>
    </motion.section>
  );
}

function ShelfEmptyCard({ title, subtitle }: { title: string; subtitle: string }) {
  return (
    <div className="shelf-empty-card">
      <img src={LOGO_SRC} alt="" />
      <div>
        <strong>{title}</strong>
        <span>{subtitle}</span>
      </div>
    </div>
  );
}

function RowHeading({ title, total, onShowMore }: { title: string; total?: number; onShowMore?: () => void }) {
  return (
    <div className="row-heading">
      <h2>{title}</h2>
      {onShowMore && (
        <button onClick={onShowMore}>
          Show More{total ? ` (${total})` : ""}
          <ChevronRight size={17} />
        </button>
      )}
    </div>
  );
}

function CatalogPage({
  genres,
  onBack,
  onOpen,
}: {
  genres: string[];
  onBack: () => void;
  onOpen: (anime: CatalogAnime) => void;
}) {
  const [filters, setFilters] = useState<CatalogFilters>({});
  const [sort, setSort] = useState("personalMatch");
  const [page, setPage] = useState(1);
  const [items, setItems] = useState<CatalogAnime[]>([]);
  const [hasNextPage, setHasNextPage] = useState(false);
  const [loading, setLoading] = useState(true);
  const currentYear = new Date().getFullYear();
  const networkSort = sort === "personalMatch" ? "trending" : sort;
  const visibleItems = useMemo(() => {
    if (sort !== "personalMatch") return items;
    return sortCatalogByPersonalMatch(items);
  }, [items, sort]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    void api.getCatalog(filters, networkSort, page)
      .then((result) => {
        if (cancelled) return;
        setItems(result.items);
        setHasNextPage(result.hasNextPage);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [filters.genre, filters.season, filters.year, filters.format, filters.status, networkSort, page]);

  function updateFilter<K extends keyof CatalogFilters>(key: K, value: CatalogFilters[K]) {
    setPage(1);
    setFilters((current) => ({ ...current, [key]: value || undefined }));
  }

  return (
    <motion.section className="catalog-browser" initial={{ opacity: 0, y: 16 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -10 }}>
      <header className="catalog-browser-header">
        <IconButton label="Back" onClick={onBack}><ArrowLeft size={21} /></IconButton>
        <div><span>Ranked for you</span><h1>Personalized catalog</h1><p>Your strongest personal matches appear first. Refine the catalog or choose another order at any time.</p></div>
      </header>
      <div className="catalog-filter-bar">
        <label className="catalog-sort-control"><span>Order</span><select value={sort} onChange={(event) => { setPage(1); setSort(event.target.value); }} aria-label="Sort catalog">
          <option value="personalMatch">Personal Match</option>
          <option value="trending">Trending</option>
          <option value="popularity">Popularity</option>
          <option value="score">AniList Score</option>
          <option value="newest">Newest</option>
          <option value="title">Title</option>
        </select></label>
        <label><span>Genre</span><select value={filters.genre || ""} onChange={(event) => updateFilter("genre", event.target.value)} aria-label="Genre">
          <option value="">All genres</option>
          {genres.map((genre) => <option value={genre} key={genre}>{genre}</option>)}
        </select></label>
        <label><span>Season</span><select value={filters.season || ""} onChange={(event) => updateFilter("season", event.target.value)} aria-label="Season">
          <option value="">All seasons</option><option value="WINTER">Winter</option><option value="SPRING">Spring</option><option value="SUMMER">Summer</option><option value="FALL">Fall</option>
        </select></label>
        <label><span>Year</span><select value={filters.year || ""} onChange={(event) => updateFilter("year", event.target.value ? Number(event.target.value) : null)} aria-label="Year">
          <option value="">All years</option>
          {Array.from({ length: 15 }, (_, index) => currentYear - index).map((year) => <option value={year} key={year}>{year}</option>)}
        </select></label>
        <label><span>Format</span><select value={filters.format || ""} onChange={(event) => updateFilter("format", event.target.value)} aria-label="Format">
          <option value="">All formats</option><option value="TV">TV</option><option value="MOVIE">Movie</option><option value="OVA">OVA</option><option value="ONA">ONA</option><option value="TV_SHORT">Short</option>
        </select></label>
        <label><span>Status</span><select value={filters.status || ""} onChange={(event) => updateFilter("status", event.target.value)} aria-label="Status">
          <option value="">All statuses</option><option value="RELEASING">Releasing</option><option value="FINISHED">Finished</option><option value="NOT_YET_RELEASED">Upcoming</option>
        </select></label>
      </div>
      <div className="catalog-grid" aria-busy={loading}>
        {loading
          ? Array.from({ length: 12 }, (_, index) => <div className="catalog-card skeleton" key={index} />)
          : visibleItems.map((anime) => (
            <button className="catalog-card" key={anime.catalogId} onClick={() => onOpen(anime)}>
              <img src={anime.coverUrl || LOGO_SRC} alt="" onError={useLogoFallback} />
              <span>{anime.title}</span>
              <small>{anime.personalMatch != null ? `${anime.personalMatch}% match` : anime.score ? `${anime.score}% score` : anime.format || "Anime"}</small>
            </button>
          ))}
      </div>
      <footer className="catalog-pagination">
        <button disabled={page === 1 || loading} onClick={() => setPage((value) => Math.max(1, value - 1))}>Previous</button>
        <span>Page {page}</span>
        <button disabled={!hasNextPage || loading} onClick={() => setPage((value) => value + 1)}>Next</button>
      </footer>
    </motion.section>
  );
}

function SearchStage({
  query,
  results,
  providerResults,
  catalogError,
  loading,
  sources,
  languageGroup,
  availability,
  selectedSource,
  selectedCatalog,
  selectedAnime,
  onQueryChange,
  onSearch,
  onLanguageChange,
  onProviderSelect,
  onProviderSourceSelect,
  onProviderHealthRetry,
  providerHealthPending,
  onSelectProviderResult,
  onSelectCatalog,
  onOpenAnime,
  onDownload,
  onToggleMyList,
  onBack,
  myList,
}: {
  query: string;
  results: CatalogAnime[];
  providerResults: Anime[];
  catalogError: AppError | null;
  loading: boolean;
  sources: Source[];
  languageGroup: "english" | "vietnamese";
  availability: ProviderAvailability[];
  selectedSource: Source | null;
  selectedCatalog: CatalogAnime | null;
  selectedAnime: Anime | null;
  onQueryChange: (query: string) => void;
  onSearch: () => void;
  onLanguageChange: (language: "english" | "vietnamese") => void;
  onProviderSelect: (option: ProviderAvailability) => void;
  onProviderSourceSelect: (source: Source) => void;
  onProviderHealthRetry: (source: Source) => void;
  providerHealthPending: string | null;
  onSelectProviderResult: (anime: Anime) => void;
  onSelectCatalog: (anime: CatalogAnime) => void;
  onOpenAnime: (anime: Anime) => void;
  onDownload: (anime: Anime) => void;
  onToggleMyList: (anime: Anime) => void;
  onBack: () => void;
  myList: Favorite[];
}) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const [mobilePreviewOpen, setMobilePreviewOpen] = useState(Boolean(selectedCatalog));
  const previewImage =
    selectedCatalog?.bannerUrl ||
    selectedAnime?.bannerUrl ||
    selectedCatalog?.coverUrl ||
    selectedAnime?.coverUrl ||
    LOGO_SRC;
  const languageSources = sources.filter((source) => source.languageGroup === languageGroup);
  const previewTitle = selectedCatalog?.title ?? selectedAnime?.title ?? "";
  const previewDescription = selectedCatalog?.description ?? selectedAnime?.synopsis ?? "";
  const previewMeta = selectedCatalog
    ? {
        episodes: selectedCatalog.totalEpisodes,
        category: selectedCatalog.genres.slice(0, 2).join(" / ") || selectedCatalog.format || "Uncategorized",
      }
    : {
        episodes: selectedAnime?.totalEpisodes,
        category: selectedAnime?.provider ?? "Provider result",
      };

  function setMobileSearchStep(previewOpen: boolean) {
    setMobilePreviewOpen(previewOpen);
    if (!window.matchMedia("(max-width: 760px)").matches) return;
    window.requestAnimationFrame(() => {
      inputRef.current
        ?.closest(".search-stage")
        ?.querySelector(".search-layout")
        ?.scrollIntoView({ behavior: "auto", block: "nearest" });
    });
  }

  useEffect(() => {
    setMobilePreviewOpen(false);
  }, [query]);

  useEffect(() => {
    if (selectedCatalog) setMobilePreviewOpen(true);
  }, [selectedCatalog]);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  return (
    <motion.section
      className="search-stage"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
    >
      <img className="search-stage-watermark" src={LOGO_SRC} alt="" aria-hidden="true" />
      <div className="search-command-panel">
        <div className="search-header">
          <IconButton label="Back" onClick={onBack}>
            <ArrowLeft size={21} />
          </IconButton>
          <div className="search-input-shell">
            <Search size={20} />
            <input
              ref={inputRef}
              type="search"
              aria-label="Search anime, films, and OVAs"
              value={query}
              placeholder="Search anime, films, OVAs..."
              onChange={(event) => onQueryChange(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") onSearch();
              }}
            />
            {loading && <Loader2 className="spin" size={19} />}
          </div>
        </div>
        <div className="search-source-row">
          <div className="language-switch" aria-label="Subtitle language">
            <button aria-pressed={languageGroup === "english"} className={languageGroup === "english" ? "active" : ""} onClick={() => onLanguageChange("english")}>English</button>
            <button aria-pressed={languageGroup === "vietnamese"} className={languageGroup === "vietnamese" ? "active" : ""} onClick={() => onLanguageChange("vietnamese")}>Vietnamese</button>
          </div>
          <div className="availability-strip" aria-label="Available providers">
            {languageSources.map((source) => {
              const option = availability.find((item) => item.provider === source.name);
              const hasDirectResult = providerResults.some((anime) => anime.provider === source.name);
              const online = source.status === "healthy" && source.capabilities.search;
              const checking = source.status === "unknown" || providerHealthPending === source.name;
              const isActive = selectedSource?.name === source.name || selectedAnime?.provider === source.name;
              const actionLabel = !source.capabilities.search
                ? "Unavailable"
                : checking
                  ? "Checking"
                  : !online
                    ? "Recheck"
                  : (hasDirectResult ? "Results" : "Search");
              return (
                <button
                  key={source.name}
                  className={isActive ? "provider-chip active" : "provider-chip"}
                  aria-label={`${source.name}: ${actionLabel}`}
                  aria-pressed={isActive}
                  disabled={!source.capabilities.search || checking}
                  title={source.failureCode || option?.failureCode || undefined}
                  onClick={() => online ? onProviderSourceSelect(source) : onProviderHealthRetry(source)}
                >
                  <i className={`health-dot ${source.status}`} />
                  <strong>{source.name}</strong>
                  <span>{actionLabel}</span>
                </button>
              );
            })}
            {!languageSources.length && (
              <span className="source-empty" role="status">
                No {languageGroup === "english" ? "English" : "Vietnamese"} providers available
              </span>
            )}
          </div>
        </div>
      </div>

      {query.trim().length < 2 && (
        <motion.section
          className="search-welcome"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.18 }}
        >
          <div className="search-welcome-copy">
            <h1>Search {selectedSource?.name ?? "a provider"}</h1>
            <p>Type at least two letters. Your query stays in place when you switch language or source.</p>
            <div className="search-suggestions" aria-label="Search suggestions">
              {["One Piece", "Naruto", "Your Name"].map((suggestion) => (
                <button
                  type="button"
                  key={suggestion}
                  onClick={() => {
                    onQueryChange(suggestion);
                    inputRef.current?.focus();
                  }}
                >
                  <Search size={15} />
                  {suggestion}
                </button>
              ))}
            </div>
          </div>
          <aside className="search-welcome-provider">
            <Film size={22} />
            <div>
              <strong>{selectedSource?.name ?? "Choose a provider"}</strong>
              <span>{selectedSource ? `${selectedSource.language} · ${providerStatusLabel(selectedSource)}` : "Select a source above to search its catalog."}</span>
            </div>
          </aside>
        </motion.section>
      )}

      {query.trim().length >= 2 && (
        <div className={`search-layout${mobilePreviewOpen ? " mobile-preview-open" : ""}`}>
          <aside className="search-results-pane">
            <div className="pane-title">
              <span>
                {selectedSource
                  ? `${selectedSource.name} Results`
                  : `No ${languageGroup === "english" ? "English" : "Vietnamese"} provider available`}
              </span>
              <strong>{providerResults.length}</strong>
            </div>
            {providerResults.map((anime, index) => {
              const active = selectedAnime && !selectedCatalog && animeKey(selectedAnime.provider, selectedAnime.id) === animeKey(anime.provider, anime.id);
              return (
                <motion.button
                  className={active ? "search-result active" : "search-result"}
                  key={animeKey(anime.provider, anime.id)}
                  onClick={() => {
                    onSelectProviderResult(anime);
                    setMobileSearchStep(true);
                  }}
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  transition={{ duration: 0.16, delay: Math.min(index * 0.01, 0.08) }}
                >
                  <img src={anime.coverUrl || LOGO_SRC} alt="" onError={useLogoFallback} />
                  <span>{anime.title}</span>
                  <small>{anime.provider} / {anime.language}</small>
                </motion.button>
              );
            })}
            {catalogError && (
              <div className="inline-status">
                <strong>{catalogError.code}</strong>
                <span>{catalogError.message}</span>
              </div>
            )}
            {loading && !providerResults.length ? (
              Array.from({ length: 9 }).map((_, index) => <div className="result-skeleton" key={index} />)
            ) : (
              !providerResults.length && (
                <EmptyPanel
                  title={!selectedSource
                    ? `No ${languageGroup === "english" ? "English" : "Vietnamese"} provider available`
                    : query.trim().length < 2 ? "any-watch" : "No results"}
                  compact
                />
              )
            )}


          </aside>

          <AnimatePresence mode="wait">
            <motion.div
              className="search-preview"
              key={selectedCatalog?.catalogId ?? (selectedAnime ? animeKey(selectedAnime.provider, selectedAnime.id) : "empty")}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.22, ease: "easeOut" }}
            >
              <button
                type="button"
                className="mobile-preview-back"
                onClick={() => setMobileSearchStep(false)}
              >
                <ArrowLeft size={18} />
                Results
              </button>
              {selectedCatalog || selectedAnime ? (
                <>
                  <div className="preview-art" style={{ backgroundImage: `url(${previewImage})` }} />
                  <div className="preview-copy">
                    <p className="eyebrow">{selectedAnime ? `${selectedAnime.provider} / ${selectedAnime.language}` : "Catalog title"}</p>
                    <h1>{previewTitle}</h1>
                    <p>{plainDescription(previewDescription) || "No synopsis is available for this title."}</p>
                    <div className="preview-meta">
                      <span><Film size={16} /> {previewMeta.episodes ? `${previewMeta.episodes} episodes` : selectedCatalog?.format || "Title"}</span>
                      <span><SlidersHorizontal size={16} /> {previewMeta.category}</span>
                    </div>
                    <div className="detail-actions">
                      <button className="primary" disabled={!selectedAnime} onClick={() => selectedAnime && onOpenAnime(selectedAnime)}>
                        <Play size={18} />
                        {selectedAnime ? "Open" : "Unavailable"}
                      </button>
                      <button onClick={() => {
                        const current = selectedAnime ?? catalogOnlyAnime(selectedCatalog!);
                        onToggleMyList(current);
                      }}>
                        {(() => {
                          const current = selectedAnime ?? catalogOnlyAnime(selectedCatalog!);
                          const isFavorite = current.isFavorite || myList.some((item) => item.animeId === animeKey(current.provider, current.id));
                          return (
                            <>
                              <Star size={18} fill={isFavorite ? "var(--red)" : "none"} style={{ color: "var(--red)" }} />
                              {isFavorite ? "In My List" : "My List"}
                            </>
                          );
                        })()}
                      </button>
                      <button
                        disabled={!selectedAnime}
                        aria-label="Choose an episode to download"
                        title="Choose an episode to download"
                        onClick={() => selectedAnime && onDownload(selectedAnime)}
                      >
                        <Download size={18} />
                        Download
                      </button>
                    </div>
                  </div>
                </>
              ) : (
                <EmptyPanel title="any-watch" />
              )}
            </motion.div>
          </AnimatePresence>
        </div>
      )}
    </motion.section>
  );
}

function HistoryPage({
  items,
  onOpen,
  onRemove,
  onBack,
  onOpenSearch,
  myList,
  onToggleFavorite,
}: {
  items: WatchHistory[];
  onOpen: (item: WatchHistory) => void;
  onRemove: (item: WatchHistory) => void;
  onBack: () => void;
  onOpenSearch: () => void;
  myList: Favorite[];
  onToggleFavorite: (item: WatchHistory) => void;
}) {
  const [filter, setFilter] = useState("");
  const [sort, setSort] = useState<ShelfSort>("recent");
  const normalized = filter.trim().toLowerCase();
  const filtered = useMemo(() => {
    const next = items.filter((item) =>
      `${item.title} ${item.provider} ${item.episodeTitle ?? ""}`.toLowerCase().includes(normalized),
    );
    next.sort((a, b) => {
      if (sort === "title") return a.title.localeCompare(b.title);
      if (sort === "provider") return a.provider.localeCompare(b.provider) || a.title.localeCompare(b.title);
      return Date.parse(b.updatedAt) - Date.parse(a.updatedAt);
    });
    return next;
  }, [items, normalized, sort]);

  return (
    <ShelfPageShell
      title="Continue Watching"
      count={items.length}
      filter={filter}
      sort={sort}
      empty="Nothing to resume yet."
      emptyDescription="Start an episode and your progress will appear here on every signed-in device."
      emptyActionLabel="Find something to watch"
      onEmptyAction={onOpenSearch}
      onBack={onBack}
      onFilterChange={setFilter}
      onSortChange={setSort}
      className="history-page"
    >
      {filtered.map((item) => (
        <HistoryCard
          item={item}
          key={item.animeId}
          onOpen={onOpen}
          onRemove={onRemove}
          isFavorite={myList.some((fav) => fav.animeId === item.animeId)}
          onToggleFavorite={onToggleFavorite}
        />
      ))}
      {!filtered.length && <EmptyPanel title={items.length ? "No matches" : "any-watch"} compact />}
    </ShelfPageShell>
  );
}

function MyListPage({
  items,
  onOpen,
  onRemove,
  onBack,
  onOpenSearch,
}: {
  items: Anime[];
  onOpen: (anime: Anime) => void;
  onRemove: (anime: Anime) => void;
  onBack: () => void;
  onOpenSearch: () => void;
}) {
  const [filter, setFilter] = useState("");
  const [sort, setSort] = useState<ShelfSort>("recent");
  const normalized = filter.trim().toLowerCase();
  const filtered = useMemo(() => {
    const next = items.filter((item) =>
      `${item.title} ${item.provider} ${item.language}`.toLowerCase().includes(normalized),
    );
    next.sort((a, b) => {
      if (sort === "provider") return a.provider.localeCompare(b.provider) || a.title.localeCompare(b.title);
      if (sort === "title") return a.title.localeCompare(b.title);
      return 0;
    });
    return next;
  }, [items, normalized, sort]);

  return (
    <ShelfPageShell
      title="My List"
      count={items.length}
      filter={filter}
      sort={sort}
      empty="Your list is ready for its first title"
      emptyDescription="Search any provider, open a title, and add it here for quick access later."
      emptyActionLabel="Search providers"
      onEmptyAction={onOpenSearch}
      onBack={onBack}
      onFilterChange={setFilter}
      onSortChange={setSort}
    >
      {filtered.map((anime) => (
        <AnimeCard
          anime={anime}
          key={`${anime.provider}:${anime.id}`}
          onOpen={onOpen}
          onRemove={onRemove}
          isFavorite={true}
          onToggleFavorite={onRemove}
        />
      ))}
      {!filtered.length && <EmptyPanel title={items.length ? "No matches" : "any-watch"} compact />}
    </ShelfPageShell>
  );
}

function AdminPage({ currentUser, onBack }: { currentUser: SessionUser; onBack: () => void }) {
  const [users, setUsers] = useState<ManagedUser[]>([]);
  const [loading, setLoading] = useState(true);
  const [message, setMessage] = useState<string | null>(null);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [role, setRole] = useState("user");
  const [creating, setCreating] = useState(false);

  async function loadUsers() {
    setLoading(true);
    try {
      setUsers(await api.listUsers());
      setMessage(null);
    } catch (err) {
      setMessage(toAppError(err, "admin-users").message);
    } finally {
      setLoading(false);
    }
  }

  useEffect(() => { void loadUsers(); }, []);

  async function createAccount(event: FormEvent) {
    event.preventDefault();
    if (creating) return;
    setCreating(true);
    setMessage(null);
    try {
      await api.createUser({ username: username.trim(), password, role });
      setUsername("");
      setPassword("");
      setRole("user");
      await loadUsers();
    } catch (err) {
      setMessage(toAppError(err, "admin-users").message);
    } finally {
      setCreating(false);
    }
  }

  return (
    <motion.section className="admin-page" initial={{ opacity: 0, y: 18 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -12 }}>
      <header className="admin-header">
        <IconButton label="Back" onClick={onBack}><ArrowLeft size={21} /></IconButton>
        <div><p className="eyebrow">Administrator</p><h1>People & access</h1><p>Create accounts and control who can use this any-watch web space.</p></div>
        <div className="admin-current-user"><ShieldCheck size={17} /><span>{currentUser.username}</span><small>Administrator</small></div>
      </header>

      <div className="admin-layout">
        <form className="admin-create-card" onSubmit={(event) => void createAccount(event)}>
          <div className="admin-card-heading"><div><p className="eyebrow">New account</p><h2>Invite a viewer</h2></div><UserPlus size={22} /></div>
          <label><span>Username</span><input value={username} onChange={(event) => setUsername(event.target.value)} minLength={3} maxLength={40} autoComplete="off" autoCapitalize="none" autoCorrect="off" spellCheck={false} /></label>
          <label><span>Temporary password</span><input type="password" value={password} onChange={(event) => setPassword(event.target.value)} minLength={10} autoComplete="new-password" /></label>
          <label><span>Access level</span><select value={role} onChange={(event) => setRole(event.target.value)}><option value="user">Viewer</option><option value="admin">Administrator</option></select></label>
          <button className="primary" disabled={creating || username.trim().length < 3 || password.length < 10}>{creating ? <Loader2 className="spin" size={17} /> : <UserPlus size={17} />}{creating ? "Creating…" : "Create account"}</button>
          <small>Passwords are hashed before storage. The password cannot be viewed again after creation.</small>
        </form>

        <section className="admin-users-card">
          <div className="admin-card-heading"><div><p className="eyebrow">Directory</p><h2>{users.length} account{users.length === 1 ? "" : "s"}</h2></div><Users size={22} /></div>
          {message && <p className="admin-message"><AlertTriangle size={16} />{message}</p>}
          {loading ? <div className="admin-loading"><Loader2 className="spin" /> Loading accounts…</div> : (
            <div className="admin-user-list">
              {users.map((user) => <AdminUserRow key={user.id} user={user} isCurrent={user.id === currentUser.id} onSaved={loadUsers} onError={setMessage} />)}
            </div>
          )}
        </section>
      </div>
    </motion.section>
  );
}

function AdminUserRow({
  user,
  isCurrent,
  onSaved,
  onError,
}: {
  user: ManagedUser;
  isCurrent: boolean;
  onSaved: () => Promise<void>;
  onError: (message: string | null) => void;
}) {
  const [username, setUsername] = useState(user.username);
  const [role, setRole] = useState(user.role);
  const [password, setPassword] = useState("");
  const [saving, setSaving] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const dirty = username.trim() !== user.username || role !== user.role || password.length > 0;

  if (user.protected) {
    return (
      <article className="admin-user-row protected admin-user-protected">
        <div className="admin-user-avatar">{user.username.slice(0, 2).toUpperCase()}</div>
        <div className="admin-protected-identity">
          <strong>{user.username}</strong>
          <small>{isCurrent ? "Current session" : "Protected administrator"}</small>
        </div>
        <div className="admin-protected-badges" aria-label="Account status">
          <span><ShieldCheck size={15} /> Protected</span>
          <span>{user.enabled ? "Active" : "Disabled"}</span>
        </div>
        <p>Managed by the private server environment. Favorites and watch history stay attached to this account in the mounted SQLite database.</p>
      </article>
    );
  }

  return (
    <article className={`admin-user-row${user.enabled ? "" : " disabled"}${user.protected ? " protected" : ""}`}>
      <div className="admin-user-avatar">{user.username.slice(0, 2).toUpperCase()}</div>
      <label className="admin-user-name">
        <span className="sr-only">Username</span>
        <input className="admin-username" value={username} disabled={user.protected} minLength={3} maxLength={40} autoComplete="off" autoCapitalize="none" autoCorrect="off" spellCheck={false} onChange={(event) => setUsername(event.target.value)} aria-label={`Username for ${user.username}`} />
        <small>{user.protected ? "Protected administrator account" : isCurrent ? "Current session" : `Created ${formatDownloadDate(user.createdAt)}`}</small>
      </label>
      <select value={role} disabled={user.protected || isCurrent} onChange={(event) => setRole(event.target.value as "admin" | "user")} aria-label={`Role for ${user.username}`}><option value="user">Viewer</option><option value="admin">Admin</option></select>
      <button
        type="button"
        className="admin-delete"
        disabled={user.protected || isCurrent || deleting || saving}
        onClick={() => {
          setDeleting(true);
          onError(null);
          void api.deleteUser(user.id)
            .then(onSaved)
            .catch((err) => onError(toAppError(err, "admin-users").message))
            .finally(() => setDeleting(false));
        }}
      >
        {deleting ? <Loader2 className="spin" size={16} /> : user.protected || isCurrent ? <ShieldCheck size={16} /> : <Trash2 size={16} />}
        {deleting ? "Deleting…" : user.protected ? "Protected" : isCurrent ? "Current" : "Delete"}
      </button>
      <input className="admin-reset-password" type="password" value={password} disabled={user.protected} onChange={(event) => setPassword(event.target.value)} placeholder={user.protected ? "Managed by server configuration" : "New password (optional)"} minLength={10} autoComplete="new-password" aria-label={`New password for ${user.username}`} />
      <button
        className="admin-save"
        disabled={user.protected || !dirty || saving || username.trim().length < 3 || (password.length > 0 && password.length < 10)}
        onClick={() => {
          setSaving(true);
          onError(null);
          void api.updateUser(user.id, { username: username.trim(), enabled: user.enabled, role, password: password || undefined })
            .then(() => { setPassword(""); return onSaved(); })
            .catch((err) => onError(toAppError(err, "admin-users").message))
            .finally(() => setSaving(false));
        }}
      >
        {saving ? <Loader2 className="spin" size={16} /> : <Check size={16} />} Save
      </button>
    </article>
  );
}

function ShelfPageShell({
  title,
  count,
  filter,
  sort,
  empty,
  emptyDescription,
  emptyActionLabel,
  onEmptyAction,
  onBack,
  onFilterChange,
  onSortChange,
  className,
  children,
}: {
  title: string;
  count: number;
  filter: string;
  sort: ShelfSort;
  empty: string;
  emptyDescription: string;
  emptyActionLabel: string;
  onEmptyAction: () => void;
  onBack: () => void;
  onFilterChange: (filter: string) => void;
  onSortChange: (sort: ShelfSort) => void;
  className?: string;
  children: ReactNode;
}) {
  return (
    <motion.section className={`grid-page ${className || ""}`} initial={{ opacity: 0, y: 18 }} animate={{ opacity: 1, y: 0 }} exit={{ opacity: 0, y: -12 }}>
      <div className="page-title-row">
        <IconButton label="Back" onClick={onBack}>
          <ArrowLeft size={21} />
        </IconButton>
        <div>
          <p className="eyebrow">{count} saved</p>
          <h1>{title}</h1>
        </div>
      </div>

      <div className="shelf-toolbar">
        <label>
          <Search size={18} />
          <input value={filter} placeholder="Filter titles..." onChange={(event) => onFilterChange(event.target.value)} />
        </label>
        <select value={sort} onChange={(event) => onSortChange(event.target.value as ShelfSort)} aria-label="Sort shelf">
          <option value="recent">Recent</option>
          <option value="title">Title</option>
          <option value="provider">Provider</option>
        </select>
      </div>

      {count ? (
        <div className="poster-grid">{children}</div>
      ) : (
        <section className="shelf-empty-state" aria-labelledby="shelf-empty-title">
          <div className="shelf-empty-icon" aria-hidden="true"><Search size={24} /></div>
          <h2 id="shelf-empty-title">{empty}</h2>
          <p>{emptyDescription}</p>
          <button className="primary" onClick={onEmptyAction}><Search size={17} />{emptyActionLabel}</button>
        </section>
      )}
    </motion.section>
  );
}

function AnimeCard({
  anime,
  onOpen,
  onRemove,
  isFavorite,
  onToggleFavorite,
}: {
  anime: Anime;
  onOpen: (anime: Anime) => void;
  onRemove?: (anime: Anime) => void;
  isFavorite?: boolean;
  onToggleFavorite?: (anime: Anime) => void;
}) {
  return (
    <motion.article whileHover={{ scale: 1.04, y: -8 }} className="poster-card">
      <button className="poster-click" onClick={() => onOpen(anime)}>
        <img src={anime.coverUrl || LOGO_SRC} alt="" loading="lazy" onError={useLogoFallback} />
        <span>{anime.title}</span>
        <small>{anime.provider} / {anime.language}</small>
      </button>
      {onToggleFavorite && (
        <button
          className="card-favorite"
          onClick={(event) => {
            event.stopPropagation();
            onToggleFavorite(anime);
          }}
          aria-label={isFavorite ? `Remove ${anime.title} from favorites` : `Add ${anime.title} to favorites`}
        >
          {isFavorite ? (
            <Star size={16} fill="var(--red)" style={{ color: "var(--red)" }} />
          ) : (
            <Star size={16} style={{ color: "var(--red)" }} />
          )}
        </button>
      )}
      {onRemove && (
        <button
          className="card-remove"
          onClick={(event) => {
            event.stopPropagation();
            onRemove(anime);
          }}
          aria-label={`Remove ${anime.title}`}
        >
          <Trash2 size={16} />
        </button>
      )}
    </motion.article>
  );
}

function HistoryCard({
  item,
  onOpen,
  onRemove,
  isFavorite,
  onToggleFavorite,
}: {
  item: WatchHistory;
  onOpen: (item: WatchHistory) => void;
  onRemove?: (item: WatchHistory) => void;
  isFavorite?: boolean;
  onToggleFavorite?: (item: WatchHistory) => void;
}) {
  const progress = item.totalSeconds > 0 ? Math.min(100, (item.positionSeconds / item.totalSeconds) * 100) : 0;
  return (
    <motion.article whileHover={{ scale: 1.035, y: -7 }} className="poster-card history">
      <button className="poster-click" onClick={() => onOpen(item)}>
        <div className="poster-image-wrapper">
          <img src={item.coverUrl || LOGO_SRC} alt="" loading="lazy" onError={useLogoFallback} />
          <div className="play-overlay">
            <Film size={28} />
          </div>
          <div className="progress watch-progress"><i style={{ width: `${progress}%` }} /></div>
        </div>
        <span>{item.title}</span>
        <small>{episodeLabel(item.episodeNumber, item.episodeTitle, " / ")}</small>
      </button>
      {onToggleFavorite && (
        <button
          className="card-favorite"
          onClick={(event) => {
            event.stopPropagation();
            onToggleFavorite(item);
          }}
          aria-label={isFavorite ? `Remove ${item.title} from favorites` : `Add ${item.title} to favorites`}
        >
          {isFavorite ? (
            <Star size={16} fill="var(--red)" style={{ color: "var(--red)" }} />
          ) : (
            <Star size={16} style={{ color: "var(--red)" }} />
          )}
        </button>
      )}
      {onRemove && (
        <button
          className="card-remove"
          onClick={(event) => {
            event.stopPropagation();
            onRemove(item);
          }}
          aria-label={`Remove ${item.title}`}
        >
          <Trash2 size={16} />
        </button>
      )}
    </motion.article>
  );
}

function chunkEpisodes(episodes: Episode[]) {
  const chunks: Episode[][] = [];
  for (let index = 0; index < episodes.length; index += EPISODE_RANGE_SIZE) {
    chunks.push(episodes.slice(index, index + EPISODE_RANGE_SIZE));
  }
  return chunks;
}

function DetailPage({
  anime,
  episodes,
  loading,
  isFavorite,
  resumeHistory,
  onBack,
  onToggleMyList,
  onPlay,
  onDownload,
  downloadStates,
}: {
  anime: Anime;
  episodes: Episode[];
  loading: boolean;
  isFavorite: boolean;
  resumeHistory?: WatchHistory;
  onBack: () => void;
  onToggleMyList: () => void;
  onPlay: (episode: Episode, startTime?: number) => void;
  onDownload: (episode: Episode) => void;
  downloadStates: Record<string, EpisodeDownloadState>;
}) {
  const [episodeQuery, setEpisodeQuery] = useState("");
  const [latestFirst, setLatestFirst] = useState(false);
  const [rangeIndex, setRangeIndex] = useState(0);
  const [highlightEpisodeNumber, setHighlightEpisodeNumber] = useState<number | null>(null);
  const episodeListRef = useRef<HTMLDivElement | null>(null);

  const sortedEpisodes = useMemo(() => {
    return [...episodes].sort((a, b) => a.number - b.number);
  }, [episodes]);

  const baseRanges = useMemo(() => chunkEpisodes(sortedEpisodes), [sortedEpisodes]);
  const safeRangeIndex = Math.min(rangeIndex, Math.max(0, baseRanges.length - 1));
  const activeRangeEpisodes = baseRanges[safeRangeIndex] ?? [];
  const visibleEpisodes = useMemo(() => {
    const normalized = episodeQuery.trim().toLowerCase();
    const source = normalized
      ? activeRangeEpisodes.filter((episode) =>
          `${episode.number} ${episode.title ?? ""}`.toLowerCase().includes(normalized),
        )
      : activeRangeEpisodes;
    return latestFirst ? [...source].reverse() : source;
  }, [activeRangeEpisodes, episodeQuery, latestFirst]);

  useEffect(() => {
    if (!baseRanges.length) {
      setRangeIndex(0);
      setHighlightEpisodeNumber(null);
      return;
    }

    const resumeNumber = resumeHistory?.episodeNumber;
    const resumeRangeIndex = resumeNumber
      ? baseRanges.findIndex((range) => range.some((episode) => episode.number === resumeNumber))
      : -1;

    setEpisodeQuery("");
    setRangeIndex(resumeRangeIndex >= 0 ? resumeRangeIndex : 0);
    setHighlightEpisodeNumber(resumeNumber ?? null);
  }, [anime.provider, anime.id, baseRanges.length, resumeHistory?.episodeNumber]);

  useEffect(() => {
    setRangeIndex((current) => Math.min(current, Math.max(0, baseRanges.length - 1)));
  }, [baseRanges.length]);

  useEffect(() => {
    if (!highlightEpisodeNumber) return undefined;
    const frame = window.requestAnimationFrame(() => {
      const node = episodeListRef.current?.querySelector<HTMLElement>(
        `[data-episode-number="${highlightEpisodeNumber}"]`,
      );
      node?.scrollIntoView({ block: "center" });
    });
    return () => window.cancelAnimationFrame(frame);
  }, [highlightEpisodeNumber, safeRangeIndex, latestFirst, episodeQuery]);

  const firstEpisode = sortedEpisodes[0];
  const latestEpisode = sortedEpisodes[sortedEpisodes.length - 1];
  const resumeEpisode = resumeHistory
    ? episodes.find((episode) => episode.number === resumeHistory.episodeNumber)
    : undefined;
  const activeRangeLabel = activeRangeEpisodes.length
    ? `${activeRangeEpisodes[0].number}-${activeRangeEpisodes[activeRangeEpisodes.length - 1].number}`
    : "0";
  const bannerDownloadEpisode = resumeEpisode ?? firstEpisode;
  const bannerDownloadState = bannerDownloadEpisode
    ? downloadStates[episodeDownloadKey(anime, bannerDownloadEpisode)]
    : undefined;

  function focusEpisode(episode: Episode) {
    const nextRange = baseRanges.findIndex((range) =>
      range.some((candidate) => candidate.number === episode.number),
    );
    if (nextRange >= 0) {
      setEpisodeQuery("");
      setRangeIndex(nextRange);
      setHighlightEpisodeNumber(episode.number);
    }
  }

  return (
    <motion.section
      className="detail-page"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
    >
      <div className="detail-page-shell">
        <div className="detail-chooser-grid" style={{ "--detail-bg": `url(${anime.bannerUrl || anime.coverUrl || LOGO_SRC})` } as React.CSSProperties}>
          <aside className="episode-range-panel">
            <div className="episode-range-top">
              <IconButton label="Back" className="detail-back-button" onClick={onBack}>
                <ArrowLeft size={21} />
              </IconButton>
              <div className="episode-range-heading">
                <p className="eyebrow">{anime.provider}</p>
                <h3>Ranges</h3>
                <span>{episodes.length} total</span>
              </div>
            </div>
            {resumeEpisode && (
              <button className="episode-resume-jump" onClick={() => focusEpisode(resumeEpisode)}>
                <Clock size={15} />
                E{resumeEpisode.number} at {formatTime(resumeHistory?.positionSeconds ?? 0)}
              </button>
            )}
            <nav className="episode-range-rail" aria-label="Episode ranges">
              {baseRanges.map((range, index) => {
                const first = range[0]?.number;
                const last = range[range.length - 1]?.number;
                const rangeHasResume = resumeEpisode
                  ? range.some((episode) => episode.number === resumeEpisode.number)
                  : false;
                return (
                  <button
                    key={`${first}-${last}`}
                    className={`episode-range-button${safeRangeIndex === index ? " active" : ""}${rangeHasResume ? " resume-range" : ""}`}
                    onClick={() => {
                      setRangeIndex(index);
                      setHighlightEpisodeNumber(null);
                    }}
                  >
                    <span>{first}-{last}</span>
                    <small>{rangeHasResume ? "Resume" : range.length}</small>
                  </button>
                );
              })}
            </nav>
          </aside>

          <section className="episode-panel episode-list-panel">
            <div className="episode-heading">
              <IconButton label="Back" className="mobile-detail-back" onClick={onBack}>
                <ArrowLeft size={21} />
              </IconButton>
              <div>
                <h3>Episodes</h3>
                <span>Range {activeRangeLabel} / {episodes.length} total</span>
              </div>
              <div className="episode-heading-actions">
                <strong>{visibleEpisodes.length} shown</strong>
              </div>
            </div>
            <div className="mobile-episode-range">
              <label>
                <span>Episode range</span>
                <select
                  value={safeRangeIndex}
                  onChange={(event) => {
                    setRangeIndex(Number(event.target.value));
                    setHighlightEpisodeNumber(null);
                  }}
                  aria-label="Episode range"
                >
                  {baseRanges.map((range, index) => {
                    const first = range[0]?.number;
                    const last = range[range.length - 1]?.number;
                    return <option key={`${first}-${last}`} value={index}>{first}-{last} · {range.length} episodes</option>;
                  })}
                </select>
              </label>
              {resumeEpisode && (
                <button className="mobile-episode-resume" onClick={() => focusEpisode(resumeEpisode)}>
                  <Clock size={15} />
                  Resume E{resumeEpisode.number}
                </button>
              )}
            </div>
            <div className="episode-toolbar">
              <label>
                <Search size={17} />
                <input
                  type="search"
                  aria-label="Find episode by number or title"
                  value={episodeQuery}
                  placeholder="Episode number or title"
                  onChange={(event) => setEpisodeQuery(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key !== "Enter" || !/^\d+$/.test(episodeQuery.trim())) return;
                    const exactEpisode = sortedEpisodes.find((episode) => episode.number === Number(episodeQuery.trim()));
                    if (!exactEpisode) return;
                    event.preventDefault();
                    focusEpisode(exactEpisode);
                  }}
                />
              </label>
              <div className="episode-sort">
                <button aria-pressed={!latestFirst} className={!latestFirst ? "active" : ""} onClick={() => setLatestFirst(false)}>First</button>
                <button aria-pressed={latestFirst} className={latestFirst ? "active" : ""} onClick={() => setLatestFirst(true)}>Latest</button>
              </div>
            </div>
            <div className="episode-list-shell">
              {loading ? <p className="empty-state">Loading episodes...</p> : null}
              {!loading && !episodes.length ? (
                <p className="empty-state">No playable episodes are currently available from {anime.provider}.</p>
              ) : null}
              {!loading && episodes.length > 0 && !visibleEpisodes.length ? (
                <p className="empty-state">No episodes match your filter.</p>
              ) : null}
              <AnimatePresence mode="popLayout">
                <motion.div
                  ref={episodeListRef}
                  className="episode-list"
                  key={`${safeRangeIndex}-${latestFirst}-${episodeQuery}`}
                  initial={{ opacity: 0, y: 15 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -15 }}
                  transition={{ duration: 0.32, ease: [0.16, 1, 0.3, 1] }}
                >
                  {visibleEpisodes.map((episode) => {
                    const isResume = resumeEpisode?.number === episode.number;
                    const highlighted = highlightEpisodeNumber === episode.number;
                    const downloadState = downloadStates[episodeDownloadKey(anime, episode)];
                    const downloadBusy = downloadState?.status === "preparing" || downloadState?.status === "downloading";
                    return (
                      <motion.div
                        className={`episode-list-row${episode.thumbnail ? " has-thumbnail" : ""}${isResume ? " watched" : ""}${highlighted ? " highlighted" : ""}`}
                        key={episode.id}
                        data-episode-number={episode.number}
                        role="button"
                        tabIndex={0}
                        onClick={() => onPlay(episode, isResume ? resumeHistory?.positionSeconds ?? 0 : 0)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter" || event.key === " ") {
                            if (event.target !== event.currentTarget) return;
                            event.preventDefault();
                            onPlay(episode, isResume ? resumeHistory?.positionSeconds ?? 0 : 0);
                          }
                        }}
                        whileHover={{ y: -2 }}
                        whileTap={{ scale: 0.995 }}
                      >
                        <span className="episode-thumb">
                          {episode.thumbnail ? <img src={episode.thumbnail} alt="" loading="lazy" onError={useLogoFallback} /> : <Play size={18} />}
                        </span>
                        <span className="episode-row-copy">
                          <strong>Episode {episode.number}</strong>
                          <small>{episodeTitleDetail(episode.title, episode.number) || "Ready to play"}</small>
                        </span>
                        {isResume && <span className="episode-resume-pill">Resume</span>}
                        <button
                          className={`episode-download-button ${downloadState?.status || "idle"}`}
                          disabled={downloadBusy}
                          aria-label={`Download Episode ${episode.number}`}
                          title={downloadState?.message || `Download Episode ${episode.number}`}
                          style={{ "--download-progress": `${downloadState?.progress ?? 0}%` } as React.CSSProperties}
                          onClick={(event) => {
                            event.stopPropagation();
                            onDownload(episode);
                          }}
                        >
                          {downloadBusy ? <Loader2 className="spin" size={17} /> : downloadState?.status === "complete" ? <Check size={17} /> : <Download size={17} />}
                        </button>
                        <Play className="episode-play-icon" size={18} fill="currentColor" />
                      </motion.div>
                    );
                  })}
                </motion.div>
              </AnimatePresence>
            </div>
          </section>

          <aside className="detail-info-panel">
            <div className="detail-poster-stage">
              <div className="detail-poster-glow" style={{ backgroundImage: `url(${anime.bannerUrl || anime.coverUrl || LOGO_SRC})` }} />
              <img src={anime.coverUrl || LOGO_SRC} alt="" onError={useLogoFallback} />
            </div>
            <div className="detail-info-copy">
              <p className="eyebrow">{anime.provider} / {anime.language}</p>
              <h2>{anime.title}</h2>
              <p>{anime.synopsis || "Episodes are loaded directly from the selected source."}</p>
              <div className="preview-meta">
                <span><Film size={16} /> {loading ? `${anime.totalEpisodes || 0} expected` : `${episodes.length} playable`}</span>
                <span><SlidersHorizontal size={16} /> {activeRangeLabel}</span>
              </div>
              <div className="detail-actions">
                {resumeEpisode && (
                  <button className="primary" onClick={() => onPlay(resumeEpisode, resumeHistory?.positionSeconds ?? 0)}>
                    <Play size={18} />
                    Resume E{resumeEpisode.number}
                  </button>
                )}
                <button className={!resumeEpisode && firstEpisode ? "primary" : ""} disabled={!firstEpisode} onClick={() => firstEpisode && onPlay(firstEpisode)}>
                  <Play size={18} />
                  {firstEpisode ? `Episode ${firstEpisode.number}` : "Unavailable"}
                </button>
                <button disabled={!latestEpisode} onClick={() => latestEpisode && onPlay(latestEpisode)}>
                  <Clock size={18} />
                  Latest
                </button>
                <button
                  className={bannerDownloadState?.status === "complete" ? "download-complete" : ""}
                  disabled={!bannerDownloadEpisode || bannerDownloadState?.status === "preparing" || bannerDownloadState?.status === "downloading"}
                  title={bannerDownloadState?.message || "Save this episode to Downloads/any-watch"}
                  onClick={() => bannerDownloadEpisode && onDownload(bannerDownloadEpisode)}
                >
                  {bannerDownloadState?.status === "preparing" || bannerDownloadState?.status === "downloading"
                    ? <Loader2 className="spin" size={18} />
                    : bannerDownloadState?.status === "complete"
                      ? <Check size={18} />
                      : <Download size={18} />}
                  {bannerDownloadState?.status === "complete"
                    ? "Downloaded"
                    : `Download E${bannerDownloadEpisode?.number ?? ""}`}
                </button>
                <button onClick={onToggleMyList}>
                  {isFavorite ? (
                    <Star size={18} fill="var(--red)" style={{ color: "var(--red)" }} />
                  ) : (
                    <Star size={18} style={{ color: "var(--red)" }} />
                  )}
                  {isFavorite ? "In My List" : "My List"}
                </button>
              </div>
              {bannerDownloadState?.message && (
                <p className={`download-status-line ${bannerDownloadState.status}`} title={bannerDownloadState.message}>
                  {bannerDownloadState.status === "complete"
                    ? bannerDownloadState.message
                    : `${Math.round(bannerDownloadState.progress)}% · ${bannerDownloadState.message}`}
                </p>
              )}
            </div>
          </aside>
        </div>
      </div>
    </motion.section>
  );
}

function VideoPlayer({
  context,
  autoSkip,
  onAutoSkipChange,
  onPlayEpisode,
  onClose,
}: {
  context: PlayerContext;
  autoSkip: boolean;
  onAutoSkipChange: (enabled: boolean) => void;
  onPlayEpisode: (episode: Episode) => Promise<void>;
  onClose: () => void;
}) {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const hlsRef = useRef<Hls | null>(null);
  const dashRef = useRef<MediaPlayerClass | null>(null);
  const subtitleTrackRefs = useRef<Array<HTMLTrackElement | null>>([]);
  const qualityRef = useRef("auto");
  const savingAtRef = useRef(0);
  const controlsTimerRef = useRef<number | null>(null);
  const skipFeedbackTimerRef = useRef<number | null>(null);
  const skippedRangesRef = useRef(new Set<string>());
  const [error, setError] = useState<string | null>(null);
  const [quality, setQuality] = useState("auto");
  const [levels, setLevels] = useState<QualityLevel[]>([]);
  const [showControls, setShowControls] = useState(true);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(context.startTime);
  const [duration, setDuration] = useState(0);
  const [volume, setVolume] = useState(1);
  const [muted, setMuted] = useState(false);
  const [skipFeedback, setSkipFeedback] = useState<{ amount?: number; label?: string; id: number } | null>(null);
  const [skipTimes, setSkipTimes] = useState<SkipTime[]>([]);
  const [skipTimingStatus, setSkipTimingStatus] = useState<"loading" | "ready" | "unavailable" | "error">("loading");
  const [switchingEpisode, setSwitchingEpisode] = useState(false);
  const streamIsHls = context.playback.streamKind === "hls";
  const streamIsDash = context.playback.streamKind === "dash";
  const subtitleTracks = context.playback.subtitles.filter((item) => item.url);
  const [subtitle, setSubtitle] = useState(subtitleTracks.length ? "0" : "off");
  const orderedEpisodes = useMemo(
    () => [...context.episodes].sort((left, right) => left.number - right.number),
    [context.episodes],
  );
  const episodeIndex = orderedEpisodes.findIndex((episode) => episode.id === context.episode.id);
  const previousEpisode = episodeIndex > 0 ? orderedEpisodes[episodeIndex - 1] : null;
  const nextEpisode = episodeIndex >= 0 && episodeIndex < orderedEpisodes.length - 1
    ? orderedEpisodes[episodeIndex + 1]
    : null;

  useEffect(() => {
    const nextSubtitle = subtitleTracks.length ? "0" : "off";
    setSubtitle(nextSubtitle);
    const frame = window.requestAnimationFrame(() => applySubtitleSelection(nextSubtitle));
    return () => window.cancelAnimationFrame(frame);
  }, [context.playback.sessionId]);

  useEffect(() => {
    let cancelled = false;
    skippedRangesRef.current.clear();
    setSkipTimes([]);
    const skipNumber = context.episode.aniskipEpisodeNumber;
    if (!context.anime.catalogId || !skipNumber) {
      setSkipTimingStatus("unavailable");
      return () => { cancelled = true; };
    }
    setSkipTimingStatus("loading");
    void api.getSkipTimes(context.anime.catalogId, skipNumber)
      .then((times) => {
        if (!cancelled) {
          setSkipTimes(times);
          setSkipTimingStatus("ready");
        }
      })
      .catch(() => {
        if (!cancelled) {
          setSkipTimes([]);
          setSkipTimingStatus("error");
        }
      });
    return () => { cancelled = true; };
  }, [context.anime.catalogId, context.episode.id, context.episode.aniskipEpisodeNumber]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    let disposed = false;
    let networkRetries = 0;
    let mediaRetries = 0;

    setError(null);
    setLevels([]);
    setQuality("auto");
    setCurrentTime(context.startTime);
    setDuration(0);
    qualityRef.current = "auto";
    hlsRef.current?.destroy();
    hlsRef.current = null;
    dashRef.current?.reset();
    dashRef.current = null;
    video.removeAttribute("src");
    video.load();

    const startPlayback = () => {
      if (disposed) return;
      try {
        if (context.startTime > 0) video.currentTime = context.startTime;
      } catch {
        // Some WebViews reject currentTime before metadata is ready.
      }
      void video.play().catch(() => undefined);
    };

    const handleNativeError = () => {
      if (!disposed && !hlsRef.current && !dashRef.current) {
        setError("The browser could not decode this stream.");
      }
    };

    video.addEventListener("error", handleNativeError);

    if (streamIsDash) {
      void import("dashjs").then((dashjs) => {
        if (disposed) return;
        const player = dashjs.MediaPlayer().create();
        dashRef.current = player;
        player.on(dashjs.MediaPlayer.events.STREAM_INITIALIZED, () => {
          if (disposed) return;
          const representations = player.getRepresentationsByType("video");
          setLevels(representations.map((representation, index) => ({
            index,
            id: representation.id,
            label: formatDashRepresentation(representation, index),
          })));
          applyDashQuality(player, representations, qualityRef.current);
          startPlayback();
        });
        player.on(dashjs.MediaPlayer.events.ERROR, (event) => {
          if (disposed) return;
          const message = "error" in event && event.error
            ? ` (${String(event.error)})`
            : "";
          setError(`The browser player failed to load this DASH stream${message}. Try the fallback player.`);
        });
        player.initialize(video, context.playback.playbackUrl, false, context.startTime || undefined);
      }).catch((loadError) => {
        if (!disposed) setError(`The DASH player could not start (${String(loadError)}).`);
      });
    } else if (streamIsHls) {
      const startNativeHls = () => {
        if (!video.canPlayType("application/vnd.apple.mpegurl")) {
          setError("This browser cannot play HLS streams.");
          return;
        }
        video.src = context.playback.playbackUrl;
        video.addEventListener("loadedmetadata", startPlayback, { once: true });
        video.load();
      };

      if (prefersNativeHls()) {
        startNativeHls();
      } else {
        void import("hls.js").then(({ default: HlsRuntime }) => {
          if (disposed) return;
          if (!HlsRuntime.isSupported()) {
            startNativeHls();
            return;
          }

          const hls = new HlsRuntime({ capLevelToPlayerSize: true, enableWorker: true });
          hlsRef.current = hls;
          hls.attachMedia(video);
          hls.loadSource(context.playback.playbackUrl);
          hls.on(HlsRuntime.Events.MANIFEST_PARSED, () => {
            if (disposed) return;
            setLevels(hls.levels.map((level, index) => ({ index, label: formatLevel(level, index) })));
            applyHlsQuality(hls, qualityRef.current);
            startPlayback();
          });
          hls.on(HlsRuntime.Events.ERROR, (_event, data) => {
            if (!data.fatal || disposed) return;
            if (data.type === HlsRuntime.ErrorTypes.MEDIA_ERROR && mediaRetries < 1) {
              mediaRetries += 1;
              hls.recoverMediaError();
            } else if (data.type === HlsRuntime.ErrorTypes.NETWORK_ERROR && networkRetries < 1) {
              networkRetries += 1;
              hls.startLoad();
            } else {
              const detail = data.details ? ` (${data.details})` : "";
              setError(`The browser player failed to load this HLS stream${detail}.`);
              hls.destroy();
            }
          });
        }).catch((loadError) => {
          if (disposed) return;
          if (video.canPlayType("application/vnd.apple.mpegurl")) {
            startNativeHls();
          } else {
            setError(`The HLS player could not start (${String(loadError)}).`);
          }
        });
      }
    } else {
      video.src = context.playback.playbackUrl;
      video.addEventListener("loadedmetadata", startPlayback, { once: true });
      video.load();
    }

    return () => {
      disposed = true;
      video.removeEventListener("error", handleNativeError);
      video.removeEventListener("loadedmetadata", startPlayback);
      hlsRef.current?.destroy();
      hlsRef.current = null;
      dashRef.current?.reset();
      dashRef.current = null;
      video.removeAttribute("src");
      video.load();
    };
  }, [context.playback.playbackUrl, context.playback.streamKind, context.startTime, streamIsDash, streamIsHls]);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    const syncState = () => {
      if (autoSkip && timingMatchesDuration(skipTimes, video.duration)) {
        const range = skipTimes.find((item) => item.skipType === "op" && video.currentTime >= item.startTime && video.currentTime < item.endTime);
        if (range) {
          const rangeKey = `${range.skipType}:${range.startTime}:${range.endTime}`;
          if (!skippedRangesRef.current.has(rangeKey)) {
            skippedRangesRef.current.add(rangeKey);
            video.currentTime = range.endTime;
            showSkipFeedback(`Skipped ${skipTypeLabel(range.skipType)}`);
          }
        }
      }
      setCurrentTime(video.currentTime || 0);
      setDuration(Number.isFinite(video.duration) ? video.duration : 0);
      setVolume(video.volume);
      setMuted(video.muted);
      setIsPlaying(!video.paused);
    };

    video.addEventListener("timeupdate", syncState);
    video.addEventListener("loadedmetadata", syncState);
    video.addEventListener("play", syncState);
    video.addEventListener("pause", syncState);
    video.addEventListener("volumechange", syncState);
    return () => {
      video.removeEventListener("timeupdate", syncState);
      video.removeEventListener("loadedmetadata", syncState);
      video.removeEventListener("play", syncState);
      video.removeEventListener("pause", syncState);
      video.removeEventListener("volumechange", syncState);
    };
  }, [autoSkip, skipTimes]);

  useEffect(() => {
    const saveInterval = window.setInterval(() => {
      void saveProgress();
    }, 15000);

    return () => window.clearInterval(saveInterval);
  }, [context.anime.id, context.episode.id]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.tagName === "INPUT" || target?.tagName === "SELECT") return;

      if (event.key === " ") {
        event.preventDefault();
        togglePlay();
      } else if (event.key === "ArrowLeft") {
        event.preventDefault();
        seekBy(-10);
      } else if (event.key === "ArrowRight") {
        event.preventDefault();
        seekBy(10);
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        setVideoVolume(Math.min(1, volume + 0.1));
      } else if (event.key === "ArrowDown") {
        event.preventDefault();
        setVideoVolume(Math.max(0, volume - 0.1));
      } else if (event.key.toLowerCase() === "m") {
        event.preventDefault();
        toggleMute();
      } else if (event.key.toLowerCase() === "f") {
        event.preventDefault();
        void toggleFullscreen();
      } else if (event.key === "Escape") {
        event.preventDefault();
        void closePlayer();
      } else if (event.key === "[") {
        event.preventDefault();
        if (previousEpisode) void changeEpisode(previousEpisode);
      } else if (event.key === "]") {
        event.preventDefault();
        if (nextEpisode) void changeEpisode(nextEpisode);
      }
      revealControls();
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [volume, muted, isPlaying, previousEpisode?.id, nextEpisode?.id, switchingEpisode]);

  useEffect(() => {
    revealControls();
  }, [isPlaying]);

  useEffect(() => () => {
    if (skipFeedbackTimerRef.current) window.clearTimeout(skipFeedbackTimerRef.current);
  }, []);

  function revealControls() {
    setShowControls(true);
    if (controlsTimerRef.current) window.clearTimeout(controlsTimerRef.current);
    if (videoRef.current && !videoRef.current.paused) {
      controlsTimerRef.current = window.setTimeout(() => setShowControls(false), 2600);
    }
  }

  function changeQuality(nextQuality: string) {
    qualityRef.current = nextQuality;
    setQuality(nextQuality);
    applyHlsQuality(hlsRef.current, nextQuality);
    if (dashRef.current) {
      applyDashQuality(
        dashRef.current,
        dashRef.current.getRepresentationsByType("video"),
        nextQuality,
      );
    }
  }

  async function saveProgress(force = false) {
    const video = videoRef.current;
    if (!video) return;
    const now = Date.now();
    if (!force && now - savingAtRef.current < 5000) return;
    savingAtRef.current = now;
    await api.saveProgress({
      animeId: animeKey(context.anime.provider, context.anime.id),
      catalogId: context.anime.catalogId ?? null,
      provider: context.anime.provider,
      title: context.anime.title,
      coverUrl: context.anime.coverUrl,
      episodeNumber: context.episode.number,
      episodeTitle: context.episode.title,
      positionSeconds: Math.floor(video.currentTime || 0),
      totalSeconds: Math.floor(Number.isFinite(video.duration) ? video.duration : 0),
      });
  }

  function togglePlay() {
    const video = videoRef.current;
    if (!video) return;
    if (video.paused) {
      void video.play().catch(() => undefined);
    } else {
      video.pause();
      void saveProgress(true);
    }
  }

  function seekBy(seconds: number) {
    const video = videoRef.current;
    if (!video) return;
    const max = Number.isFinite(video.duration) ? video.duration : video.currentTime + seconds;
    video.currentTime = Math.max(0, Math.min(max, video.currentTime + seconds));
    setCurrentTime(video.currentTime);
    showSkipFeedback(undefined, seconds);
    void saveProgress(true);
  }

  function showSkipFeedback(label?: string, amount?: number) {
    setSkipFeedback({ amount, label, id: Date.now() });
    if (skipFeedbackTimerRef.current) window.clearTimeout(skipFeedbackTimerRef.current);
    skipFeedbackTimerRef.current = window.setTimeout(() => setSkipFeedback(null), 850);
  }

  function setVideoVolume(nextVolume: number) {
    const video = videoRef.current;
    if (!video) return;
    video.volume = nextVolume;
    if (nextVolume > 0) video.muted = false;
  }

  function toggleMute() {
    const video = videoRef.current;
    if (!video) return;
    video.muted = !video.muted;
  }

  function changeSubtitle(value: string) {
    setSubtitle(value);
    applySubtitleSelection(value);
  }

  function applySubtitleSelection(value: string) {
    subtitleTrackRefs.current.forEach((track, index) => {
      if (track) track.track.mode = value === String(index) ? "showing" : "disabled";
    });
  }

  async function toggleFullscreen() {
    const root = videoRef.current?.parentElement;
    if (!root) return;
    if (document.fullscreenElement) {
      await document.exitFullscreen().catch(() => undefined);
    } else {
      await root.requestFullscreen().catch(() => undefined);
    }
  }

  async function togglePictureInPicture() {
    const video = videoRef.current;
    if (!video) return;
    const pipDocument = document as Document & {
      pictureInPictureElement?: Element | null;
      exitPictureInPicture?: () => Promise<void>;
    };
    const pipVideo = video as HTMLVideoElement & {
      requestPictureInPicture?: () => Promise<unknown>;
      webkitSetPresentationMode?: (mode: "inline" | "picture-in-picture") => void;
      webkitPresentationMode?: string;
    };

    if (pipDocument.pictureInPictureElement && pipDocument.exitPictureInPicture) {
      await pipDocument.exitPictureInPicture().catch(() => undefined);
    } else if (pipVideo.requestPictureInPicture) {
      await pipVideo.requestPictureInPicture().catch(() => undefined);
    } else if (pipVideo.webkitSetPresentationMode) {
      pipVideo.webkitSetPresentationMode(
        pipVideo.webkitPresentationMode === "picture-in-picture" ? "inline" : "picture-in-picture",
      );
    }
  }

  async function closePlayer() {
    await saveProgress(true);
    onClose();
  }

  async function changeEpisode(episode: Episode) {
    if (switchingEpisode || episode.id === context.episode.id) return;
    setSwitchingEpisode(true);
    await saveProgress(true).catch(() => undefined);
    try {
      await onPlayEpisode(episode);
    } finally {
      setSwitchingEpisode(false);
    }
  }

  const progress = duration > 0 ? (currentTime / duration) * 100 : 0;
  const markerDuration = duration > 0
    ? duration
    : Math.max(0, ...skipTimes.map((item) => item.endTime));
  const timelineSkipTimes = markerDuration > 0
    ? skipTimes.filter((item) => item.startTime < markerDuration && item.endTime > 0)
    : [];

  return (
    <motion.div
      className={showControls ? "player-overlay controls-visible" : "player-overlay"}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
      onMouseMove={revealControls}
      onClick={revealControls}
    >
      <video
        ref={videoRef}
        autoPlay
        onTimeUpdate={() => void saveProgress()}
        onPause={() => void saveProgress(true)}
        onEnded={() => void saveProgress(true)}
        onDoubleClick={() => void toggleFullscreen()}
      >
        {subtitleTracks.map((item, index) => (
          <track
            key={item.url}
            ref={(track) => { subtitleTrackRefs.current[index] = track; }}
            kind="subtitles"
            src={item.url}
            srcLang={languageCode(item.language)}
            label={item.language}
            default={index === 0}
            onLoad={(event) => {
              event.currentTarget.track.mode = subtitle === String(index) ? "showing" : "disabled";
            }}
          />
        ))}
      </video>

      <div className="player-top">
        <div className="player-leading-controls">
          <button onClick={() => void closePlayer()} aria-label="Back to episodes" title="Back to episodes">
            <ArrowLeft size={20} />
          </button>
          <button onClick={() => void togglePictureInPicture()} aria-label="Picture in Picture" title="Picture in Picture">
            <PictureInPicture2 size={20} />
          </button>
        </div>
      </div>

      <div className="player-volume-dock">
        <button onClick={toggleMute} aria-label={muted ? "Unmute" : "Mute"}>
          {muted || volume === 0 ? <VolumeX size={20} /> : <Volume2 size={20} />}
        </button>
        <input
          className="volume-slider"
          type="range"
          min={0}
          max={1}
          step={0.05}
          value={muted ? 0 : volume}
          onChange={(event) => setVideoVolume(Number(event.target.value))}
          aria-label="Volume"
        />
      </div>

      <div className="player-center" role="group" aria-label="Playback controls">
        <button
          className="episode-transport"
          onClick={() => previousEpisode && void changeEpisode(previousEpisode)}
          disabled={!previousEpisode || switchingEpisode}
          aria-label="Previous episode"
          aria-busy={switchingEpisode}
          data-state={switchingEpisode ? "loading" : undefined}
          title="Previous episode ([)"
        >
          <SkipBack size={30} />
        </button>
        <button className="seek-transport" onClick={() => seekBy(-10)} aria-label="Back 10 seconds" title="Back 10 seconds">
          <RotateCcw size={34} />
          <span className="seek-seconds" aria-hidden="true">10</span>
        </button>
        <button className="play-ring" onClick={togglePlay} aria-label={isPlaying ? "Pause" : "Play"}>
          {isPlaying ? <Pause size={34} /> : <Play size={34} />}
        </button>
        <button className="seek-transport" onClick={() => seekBy(10)} aria-label="Forward 10 seconds" title="Forward 10 seconds">
          <RotateCw size={34} />
          <span className="seek-seconds" aria-hidden="true">10</span>
        </button>
        <button
          className="episode-transport"
          onClick={() => nextEpisode && void changeEpisode(nextEpisode)}
          disabled={!nextEpisode || switchingEpisode}
          aria-label="Next episode"
          aria-busy={switchingEpisode}
          data-state={switchingEpisode ? "loading" : undefined}
          title="Next episode (])"
        >
          <SkipForward size={30} />
        </button>
        <AnimatePresence mode="wait">
          {skipFeedback ? (
            <motion.div
              key={skipFeedback.id}
              className="player-skip-feedback"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.14 }}
              role="status"
              aria-live="polite"
            >
              {skipFeedback.label ?? `${(skipFeedback.amount ?? 0) > 0 ? "+" : "−"}${Math.abs(skipFeedback.amount ?? 0)} seconds`}
            </motion.div>
          ) : null}
        </AnimatePresence>
      </div>

      <div className="player-bottom">
        {error && (
          <div className="player-error-fallback">
            <span>{error}</span>
          </div>
        )}
        <div className="player-control-row">
          <div className="player-now-playing">
            <span>{context.anime.provider}</span>
            <strong>{context.anime.title}</strong>
            <small>{episodeLabel(context.episode.number, context.episode.title)}</small>
          </div>
          <div className="player-utility-pill">
            <button onClick={() => void toggleFullscreen()} aria-label="Toggle fullscreen" title="Toggle fullscreen">
              <Maximize2 size={18} />
            </button>
            <label title="Video quality">
              <span>Quality</span>
            <select value={quality} onChange={(event) => changeQuality(event.target.value)} disabled={(!streamIsHls && !streamIsDash) || !levels.length}>
              <option value="auto">Auto</option>
              {levels.map((level) => <option value={String(level.index)} key={level.index}>{level.label}</option>)}
            </select>
            </label>
            <button
              className="player-auto-skip-toggle"
              type="button"
              role="switch"
              aria-checked={autoSkip}
              aria-busy={skipTimingStatus === "loading"}
              data-state={skipTimingStatus === "error" ? "error" : skipTimingStatus === "ready" ? "success" : skipTimingStatus}
              disabled={skipTimingStatus === "loading"}
              aria-label="Toggle skip intro"
              title={skipTimingStatusLabel(skipTimingStatus, skipTimes.length)}
              onClick={() => onAutoSkipChange(!autoSkip)}
            >
              Skip intro <span>{autoSkip ? "On" : "Off"}</span>
            </button>
            {subtitleTracks.length > 0 && (
              <label title="Subtitles">
                <span>Subtitles</span>
                <select value={subtitle} onChange={(event) => changeSubtitle(event.target.value)}>
                  <option value="off">Off</option>
                  {subtitleTracks.map((track, index) => <option value={String(index)} key={track.url}>{track.language}</option>)}
                </select>
              </label>
            )}
          </div>
        </div>
        <div className="player-timeline">
          <span>{formatTime(currentTime)}</span>
          <div className="player-progress-shell">
            <div className="player-skip-markers" role="list" aria-label="Opening and ending timeline markers">
              {timelineSkipTimes.map((item) => {
                const start = Math.max(0, Math.min(100, item.startTime / markerDuration * 100));
                const end = Math.max(start, Math.min(100, item.endTime / markerDuration * 100));
                return (
                  <span
                    key={`${item.skipType}:${item.startTime}:${item.endTime}`}
                    className={`player-skip-marker ${item.skipType}`}
                    role="listitem"
                    aria-label={`${skipTypeDisplayLabel(item.skipType)}, ${formatTime(item.startTime)} to ${formatTime(item.endTime)}`}
                    style={{ insetInlineStart: `${start}%`, width: `${Math.max(0.35, end - start)}%` }}
                    title={`${skipTypeDisplayLabel(item.skipType)} · ${formatTime(item.startTime)}–${formatTime(item.endTime)}`}
                  >
                    <span className="sr-only">{skipTypeDisplayLabel(item.skipType)}</span>
                  </span>
                );
              })}
            </div>
            <input
              className="player-progress"
              type="range"
              min={0}
              max={duration || 0}
              step={1}
              value={Math.min(currentTime, duration || currentTime)}
              style={{ "--progress": `${progress}%` } as React.CSSProperties}
              onChange={(event) => {
                const video = videoRef.current;
                if (!video) return;
                video.currentTime = Number(event.target.value);
                setCurrentTime(video.currentTime);
              }}
              onMouseUp={() => void saveProgress(true)}
            />
          </div>
          <span>-{formatTime(Math.max(0, duration - currentTime))}</span>
        </div>
      </div>
    </motion.div>
  );
}

function prefersNativeHls() {
  const userAgent = navigator.userAgent;
  const isSafari = /Safari\//.test(userAgent);
  const isChromium = /(Chrome|Chromium|CriOS|Edg|OPR)\//.test(userAgent);
  return isSafari && !isChromium;
}

function EmptyPanel({ title, compact = false }: { title: string; compact?: boolean }) {
  return (
    <div className={compact ? "empty-panel compact" : "empty-panel"}>
      <h2>{title}</h2>
    </div>
  );
}

function useLogoFallback(event: SyntheticEvent<HTMLImageElement>) {
  const image = event.currentTarget;
  if (image.dataset.fallbackApplied === "true") return;
  image.dataset.fallbackApplied = "true";
  image.src = LOGO_SRC;
  image.classList.add("media-fallback");
}

function IconButton({
  label,
  className,
  onClick,
  children,
}: {
  label: string;
  className?: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button className={className ? `icon-button ${className}` : "icon-button"} onClick={onClick} aria-label={label} title={label}>
      {children}
    </button>
  );
}

function historyToAnime(item: WatchHistory, myList: Favorite[]): Anime {
  return {
    id: item.animeId.includes(":") ? item.animeId.split(":").slice(1).join(":") : item.animeId,
    provider: item.provider,
    catalogId: item.catalogId ?? null,
    title: item.title,
    coverUrl: item.coverUrl,
    bannerUrl: null,
    language: "History",
    totalEpisodes: null,
    synopsis: null,
    isFavorite: myList.some((favorite) => favorite.animeId === item.animeId),
  };
}

function catalogToAnime(catalog: CatalogAnime, providerAnime: Anime): Anime {
  return {
    ...providerAnime,
    catalogId: catalog.catalogId,
    title: catalog.title || providerAnime.title,
    coverUrl: catalog.coverUrl || providerAnime.coverUrl,
    bannerUrl: catalog.bannerUrl || providerAnime.bannerUrl,
    totalEpisodes: providerAnime.totalEpisodes ?? catalog.totalEpisodes,
    synopsis: catalog.description || providerAnime.synopsis,
  };
}

function exactCatalogMatch(catalog: CatalogAnime[], anime: Anime): CatalogAnime | null {
  return exactCatalogTitleMatch(catalog, anime.title);
}

function exactCatalogTitleMatch(catalog: CatalogAnime[], title: string): CatalogAnime | null {
  const normalizedTitle = normalizeCatalogTitle(title);
  if (!normalizedTitle) return null;
  const matches = catalog.filter((item) => [item.title, item.nativeTitle, ...(item.synonyms ?? [])]
    .some((candidate) => normalizeCatalogTitle(candidate ?? "") === normalizedTitle));
  return matches.length === 1 ? matches[0] : null;
}

function normalizeCatalogTitle(title: string) {
  return title
    .normalize("NFKD")
    .toLowerCase()
    .replace(/[\u0300-\u036f]/g, "")
    .replace(/[^a-z0-9]+/g, " ")
    .trim();
}

function catalogOnlyAnime(catalog: CatalogAnime): Anime {
  return {
    id: String(catalog.catalogId),
    catalogId: catalog.catalogId,
    provider: "Catalog",
    title: catalog.title,
    coverUrl: catalog.coverUrl,
    bannerUrl: catalog.bannerUrl,
    language: "Catalog",
    totalEpisodes: catalog.totalEpisodes,
    synopsis: catalog.description,
    isFavorite: false,
  };
}

function firstSearchableSource(sources: Source[], group: "english" | "vietnamese") {
  return sources.find(
    (source) => source.languageGroup === group && source.status === "healthy" && source.capabilities.search,
  ) ?? null;
}

function episodeDownloadKey(anime: Anime, episode: Episode) {
  return `${animeKey(anime.provider, anime.id)}:${episode.id}`;
}

function plainDescription(value?: string | null): string {
  if (!value) return "";
  const withBreaks = value.replace(/<br\s*\/?>/gi, "\n");
  if (typeof DOMParser === "undefined") {
    return withBreaks.replace(/<[^>]+>/g, "").replace(/&lt;br\s*\/??&gt;/gi, "\n").trim();
  }
  const document = new DOMParser().parseFromString(withBreaks, "text/html");
  return (document.body.textContent || "")
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<[^>]+>/g, "")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

function sortCatalogByPersonalMatch(items: CatalogAnime[]): CatalogAnime[] {
  return [...items].sort((left, right) => {
    const matchDifference = (right.personalMatch ?? right.score ?? 0) - (left.personalMatch ?? left.score ?? 0);
    if (matchDifference !== 0) return matchDifference;
    const scoreDifference = (right.score ?? 0) - (left.score ?? 0);
    if (scoreDifference !== 0) return scoreDifference;
    return left.title.localeCompare(right.title);
  });
}

function detailPatch(details: AnimeDetails): Partial<Anime> {
  const patch: Partial<Anime> = {};
  if (nonEmpty(details.coverUrl)) patch.coverUrl = details.coverUrl!.trim();
  if (nonEmpty(details.bannerUrl)) patch.bannerUrl = details.bannerUrl!.trim();
  if (typeof details.totalEpisodes === "number" && details.totalEpisodes > 0) {
    patch.totalEpisodes = details.totalEpisodes;
  }
  if (nonEmpty(details.synopsis)) patch.synopsis = details.synopsis!.trim();
  return patch;
}

function mergeAnimeDetails(anime: Anime, patch: Partial<Anime>): Anime {
  return {
    ...anime,
    coverUrl: nonEmpty(patch.coverUrl) ? patch.coverUrl! : anime.coverUrl,
    bannerUrl: nonEmpty(patch.bannerUrl) ? patch.bannerUrl : anime.bannerUrl,
    totalEpisodes: patch.totalEpisodes ?? anime.totalEpisodes,
    synopsis: nonEmpty(patch.synopsis) ? patch.synopsis : anime.synopsis,
  };
}

function nonEmpty(value?: string | null) {
  return typeof value === "string" && value.trim().length > 0;
}

function findHistoryForAnime(anime: Anime, history: WatchHistory[]) {
  const key = animeKey(anime.provider, anime.id);
  return history.find((item) => item.animeId === key);
}

function loadSavedSourceName() {
  try {
    return localStorage.getItem(SOURCE_STORAGE_KEY) ?? localStorage.getItem("any-watch:selected-source");
  } catch {
    return null;
  }
}

function saveSourceName(sourceName: string) {
  try {
    localStorage.setItem(SOURCE_STORAGE_KEY, sourceName);
  } catch {
    // localStorage can be unavailable in restricted WebView contexts.
  }
}

function loadSavedTheme(): AppTheme {
  try {
    const saved = localStorage.getItem(THEME_STORAGE_KEY) ?? localStorage.getItem("any-watch:theme");
    if (saved === "obsidian" || saved === "oled" || saved === "ember" || saved === "crimson" || saved === "system") return saved;
  } catch {
    // localStorage can be unavailable in restricted WebView contexts.
  }
  return "obsidian";
}

function saveTheme(theme: AppTheme) {
  try {
    localStorage.setItem(THEME_STORAGE_KEY, theme);
  } catch {
    // localStorage can be unavailable in restricted WebView contexts.
  }
}

function loadSavedScale(): AppScale {
  try {
    const saved = localStorage.getItem(APP_SCALE_STORAGE_KEY) ?? localStorage.getItem("any-watch:scale");
    if (saved === "compact" || saved === "comfortable" || saved === "large" || saved === "tv") return saved;
  } catch {
    // localStorage can be unavailable in restricted WebView contexts.
  }
  return "comfortable";
}

function saveScale(scale: AppScale) {
  try {
    localStorage.setItem(APP_SCALE_STORAGE_KEY, scale);
  } catch {
    // localStorage can be unavailable in restricted WebView contexts.
  }
}

function loadSavedFont(): AppFont {
  try {
    const saved = localStorage.getItem(APP_FONT_STORAGE_KEY) ?? localStorage.getItem("any-watch:font");
    if (saved === "manrope" || saved === "noto" || saved === "system") return saved;
  } catch {
    // localStorage can be unavailable in restricted WebView contexts.
  }
  return "manrope";
}

function saveFont(font: AppFont) {
  try {
    localStorage.setItem(APP_FONT_STORAGE_KEY, font);
  } catch {
    // localStorage can be unavailable in restricted WebView contexts.
  }
}

function loadSavedAutoSkip() {
  try {
    const saved = localStorage.getItem(SKIP_INTRO_STORAGE_KEY)
      ?? localStorage.getItem("any-watch:auto-skip")
      ?? localStorage.getItem("ani-desk:auto-skip");
    return saved === null ? true : saved !== "false";
  } catch {
    return true;
  }
}

function saveAutoSkip(enabled: boolean) {
  try {
    localStorage.setItem(SKIP_INTRO_STORAGE_KEY, String(enabled));
  } catch {
    // localStorage can be unavailable in restricted WebView contexts.
  }
}

function skipTypeLabel(skipType: string) {
  if (skipType === "op") return "opening";
  if (skipType === "ed") return "ending";
  if (skipType === "recap") return "recap";
  return "segment";
}

function skipTypeDisplayLabel(skipType: string) {
  if (skipType === "op") return "Opening";
  if (skipType === "ed") return "Ending";
  if (skipType === "recap") return "Recap";
  return "Skip segment";
}

function skipTimingStatusLabel(status: "loading" | "ready" | "unavailable" | "error", count: number) {
  if (status === "loading") return "Loading timing data";
  if (status === "error") return "Timing unavailable";
  if (status === "unavailable") return "No catalog match";
  return count ? `${count} marked segment${count === 1 ? "" : "s"}` : "No marked segments";
}

function timingMatchesDuration(times: SkipTime[], duration: number) {
  if (!Number.isFinite(duration) || duration <= 0) return false;
  const expected = times.find((item) => item.episodeLength && Number.isFinite(item.episodeLength))?.episodeLength;
  return expected == null || Math.abs(duration - expected) <= Math.max(20, expected * 0.03);
}

function applyHlsQuality(hls: Hls | null, quality: string) {
  if (!hls) return;
  if (quality === "auto") {
    hls.currentLevel = -1;
    return;
  }
  const level = Number(quality);
  if (Number.isInteger(level)) hls.currentLevel = level;
}

function formatDashRepresentation(representation: Representation, index: number) {
  if (representation.height) return `${representation.height}p`;
  if (representation.bitrateInKbit) return `${Math.round(representation.bitrateInKbit)} kbps`;
  return `Quality ${index + 1}`;
}

function applyDashQuality(
  player: MediaPlayerClass | null,
  representations: Representation[],
  quality: string,
) {
  if (!player) return;
  if (quality === "auto") {
    player.updateSettings({ streaming: { abr: { autoSwitchBitrate: { video: true } } } });
    return;
  }
  const representation = representations[Number(quality)];
  if (!representation) return;
  player.updateSettings({ streaming: { abr: { autoSwitchBitrate: { video: false } } } });
  player.setRepresentationForTypeById("video", representation.id, true);
}

function formatLevel(level: { height?: number; bitrate?: number; name?: string }, index: number) {
  if (level.height) return `${level.height}p`;
  if (level.name) return level.name;
  if (level.bitrate) return `${Math.round(level.bitrate / 1000)} kbps`;
  return `Level ${index + 1}`;
}

function languageCode(language: string) {
  const normalized = language.toLowerCase();
  if (normalized.startsWith("vi")) return "vi";
  if (normalized.startsWith("en")) return "en";
  return normalized.slice(0, 2) || "und";
}

function formatTime(seconds: number) {
  if (!Number.isFinite(seconds) || seconds <= 0) return "0:00";
  const whole = Math.floor(seconds);
  const hours = Math.floor(whole / 3600);
  const minutes = Math.floor((whole % 3600) / 60);
  const secs = whole % 60;
  if (hours > 0) {
    return `${hours}:${String(minutes).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
  }
  return `${minutes}:${String(secs).padStart(2, "0")}`;
}

function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB"];
  const index = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1);
  const value = bytes / 1024 ** index;
  return `${value >= 10 || index === 0 ? Math.round(value) : value.toFixed(1)} ${units[index]}`;
}

function formatDownloadDate(value: string) {
  const timestamp = Date.parse(value);
  if (!Number.isFinite(timestamp)) return "Saved locally";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    year: new Date(timestamp).getFullYear() === new Date().getFullYear() ? undefined : "numeric",
  }).format(timestamp);
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

function providerFailureMakesOffline(error: AppError) {
  return new Set([
    "PROVIDER_UNAVAILABLE",
    "PROVIDER_CAPTCHA",
    "PROVIDER_RATE_LIMITED",
    "NETWORK_TIMEOUT",
    "STREAM_NOT_FOUND",
    "STREAM_FORBIDDEN",
  ]).has(error.code);
}

function toAppError(error: unknown, operation: string): AppError {
  if (error && typeof error === "object") {
    const value = error as Partial<AppError>;
    if (typeof value.code === "string" && typeof value.message === "string") {
      return {
        code: value.code,
        message: value.message,
        provider: value.provider ?? null,
        operation: value.operation || operation,
        retryable: Boolean(value.retryable),
        correlationId: value.correlationId || crypto.randomUUID(),
        technical: value.technical ?? null,
      };
    }
  }
  return {
    code: "UNEXPECTED_ERROR",
    message: errorMessage(error),
    operation,
    retryable: true,
    correlationId: crypto.randomUUID(),
  };
}

export default App;
