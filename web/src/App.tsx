import type Hls from "hls.js";
import type { MediaPlayerClass, Representation } from "dashjs";
import {
  ArrowLeft,
  AlertTriangle,
  Bookmark,
  Check,
  ChevronLeft,
  ChevronRight,
  Clock,
  Compass,
  Copy,
  Download,
  Eye,
  EyeOff,
  Film,
  Flame,
  Gamepad2,
  Grid,
  HardDrive,
  Heart,
  House,
  LayoutGrid,
  List,
  Loader2,
  LogIn,
  LogOut,
  Maximize2,
  Music,
  Newspaper,
  Pause,
  PictureInPicture2,
  Play,
  Plus,
  RefreshCw,
  RotateCcw,
  RotateCw,
  Search,
  Settings2,
  Share2,
  ShieldCheck,
  SkipBack,
  SkipForward,
  SlidersHorizontal,
  Sparkles,
  Star,
  ThumbsDown,
  ThumbsUp,
  Trash2,
  Tv,
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
import {
  getCachedYouTubeMetadata,
  saveCachedYouTubeMetadata,
  loadCachedYouTubeCatalog,
  saveCachedYouTubeCatalog,
} from "./youtube-storage";
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
  MediaMetadata,
  Playback,
  PlayerContext,
  ProviderAvailability,
  Source,
  SessionUser,
  SkipTime,
  TorrentSearchResult,
  TorrentTask,
  WatchHistory,
  YouTubeTopic,
} from "./types";

const SOURCE_STORAGE_KEY = "any-watch:selected-source";
const THEME_STORAGE_KEY = "any-watch:theme";
const APP_SCALE_STORAGE_KEY = "any-watch:scale";
const APP_FONT_STORAGE_KEY = "any-watch:font";
const SKIP_INTRO_STORAGE_KEY = "any-watch:skip-intro";
const EPISODE_RANGE_SIZE = 50;
const LOGO_SRC = "/logo.png";

function isSourceActive(source: Source): boolean {
  return source.status === "healthy" && Boolean(source.capabilities?.search);
}

function serverLabel(providerName: string, sources: Source[]): string {
  if (providerName === "Invidious") return "YouTube";
  const source = sources.find((s) => s.name === providerName);
  const group = source?.languageGroup ?? "english";
  const activeSources = sources.filter(
    (s) => s.languageGroup === group && s.languageGroup !== "youtube" && isSourceActive(s),
  );
  const index = activeSources.findIndex((s) => s.name === providerName);
  if (index >= 0) return `Server ${index + 1}`;
  const fallbackSources = sources.filter(
    (s) => s.languageGroup === group && s.languageGroup !== "youtube" && s.status !== "unavailable" && Boolean(s.capabilities?.search),
  );
  const fallbackIndex = fallbackSources.findIndex((s) => s.name === providerName);
  return fallbackIndex >= 0 ? `Server ${fallbackIndex + 1}` : providerName;
}
const fadeUpVariant = {
  hidden: { opacity: 0, y: 18 },
  show: { opacity: 1, y: 0 },
};

type Route = "home" | "my-list" | "continue" | "admin" | "search" | "youtube" | "detail" | "catalog" | "settings" | "download" | "donate";

function routeFromLocation(): Route {
  switch (window.location.pathname.replace(/\/+$/, "") || "/") {
    case "/search": return "search";
    case "/youtube": return "youtube";
    case "/downloads": return "download";
    case "/donate": return "donate";
    case "/my-list": return "my-list";
    case "/continue": return "continue";
    case "/catalog": return "catalog";
    case "/settings": return "settings";
    case "/admin": return "admin";
    default: return "home";
  }
}

function pathForRoute(route: Route): string {
  switch (route) {
    case "search": return "/search";
    case "youtube": return "/youtube";
    case "download": return "/downloads";
    case "donate": return "/donate";
    case "my-list": return "/my-list";
    case "continue": return "/continue";
    case "catalog": return "/catalog";
    case "settings": return "/settings";
    case "admin": return "/admin";
    case "detail": return "/watch";
    default: return "/";
  }
}

type AppTheme =
  | "obsidian"
  | "oled"
  | "ember"
  | "crimson"
  | "tokyo"
  | "cyberpunk"
  | "emerald"
  | "amethyst"
  | "sunset"
  | "nordic"
  | "system";
type AppScale = "compact" | "comfortable" | "large" | "tv";
type AppFont = "manrope" | "noto" | "jakarta" | "outfit" | "vietnam" | "mono" | "system";
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

function preloadImages(urls: (string | null | undefined)[]) {
  if (typeof window === "undefined") return;
  for (const url of urls) {
    if (url && typeof url === "string" && (url.startsWith("http://") || url.startsWith("https://") || url.startsWith("/"))) {
      const img = new Image();
      img.src = url;
    }
  }
}

function getCachedEpisodes(provider: string, animeId: string): Episode[] | null {
  try {
    const key = `any-watch:episodes-cache-v2:${provider}:${animeId}`;
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (parsed && Array.isArray(parsed.episodes) && typeof parsed.timestamp === "number") {
      if (Date.now() - parsed.timestamp < 2 * 60 * 60 * 1000 && parsed.episodes.length > 0) {
        return parsed.episodes;
      }
    }
  } catch {
    // Ignore cache error
  }
  return null;
}

function saveCachedEpisodes(provider: string, animeId: string, episodes: Episode[]) {
  try {
    if (episodes.length > 0) {
      const key = `any-watch:episodes-cache-v2:${provider}:${animeId}`;
      localStorage.setItem(key, JSON.stringify({ timestamp: Date.now(), episodes }));
      const thumbs = episodes.map((ep) => ep.thumbnail).filter(Boolean);
      preloadImages(thumbs);
    }
  } catch {
    // Ignore localStorage error
  }
}

function getCachedAnimeDetails(provider: string, animeId: string): Partial<Anime> | null {
  try {
    const key = `any-watch:details-cache-v2:${provider}:${animeId}`;
    const raw = localStorage.getItem(key);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (parsed && parsed.details && typeof parsed.timestamp === "number") {
      if (Date.now() - parsed.timestamp < 24 * 60 * 60 * 1000) {
        return parsed.details;
      }
    }
  } catch {
    // Ignore cache error
  }
  return null;
}

function saveCachedAnimeDetails(provider: string, animeId: string, details: Partial<Anime>) {
  try {
    const key = `any-watch:details-cache-v2:${provider}:${animeId}`;
    localStorage.setItem(key, JSON.stringify({ timestamp: Date.now(), details }));
    if (details.coverUrl || details.bannerUrl) {
      preloadImages([details.coverUrl, details.bannerUrl]);
    }
  } catch {
    // Ignore localStorage error
  }
}

export type YouTubeCatalogsData = {
  trending: Anime[];
  music: Anime[];
  films: Anime[];
  anime: Anime[];
  gaming: Anime[];
  news: Anime[];
  loading: Record<string, boolean>;
  errors: Record<string, string | null>;
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
  const [youtubeQuery, setYoutubeQuery] = useState("");
  const [youtubeResults, setYoutubeResults] = useState<Anime[]>([]);
  const [youtubeSelection, setYoutubeSelection] = useState<Anime | null>(null);
  const [youtubeLoading, setYoutubeLoading] = useState(false);
  const [youtubeTopic, setYoutubeTopic] = useState<YouTubeTopic>("All");
  const [youtubeFeed, setYoutubeFeed] = useState<Anime[]>([]);
  const [youtubeFeedLoading, setYoutubeFeedLoading] = useState(false);
  const [youtubeFeedError, setYoutubeFeedError] = useState<string | null>(null);
  const [youtubeCatalogs, setYoutubeCatalogs] = useState<YouTubeCatalogsData>({
    trending: [],
    music: [],
    films: [],
    anime: [],
    gaming: [],
    news: [],
    loading: {},
    errors: {},
  });
  const [youtubeViewMode, setYoutubeViewMode] = useState<"dashboard" | "grid">("dashboard");
  const [youtubeRelated, setYoutubeRelated] = useState<Anime[]>([]);
  const [youtubeRelatedLoading, setYoutubeRelatedLoading] = useState(false);
  const [youtubeRelatedError, setYoutubeRelatedError] = useState<string | null>(null);
  const [youtubeWatchMode, setYoutubeWatchMode] = useState(false);
  const [youtubeEmbedPlaying, setYoutubeEmbedPlaying] = useState(false);
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
  const [route, setRoute] = useState<Route>(routeFromLocation);
  const [routeStack, setRouteStack] = useState<Route[]>([]);
  const detailCacheRef = useRef<Record<string, Partial<Anime>>>({});
  const availabilityCacheRef = useRef(new Map<string, { expiresAt: number; items: ProviderAvailability[] }>());
  const catalogSearchCacheRef = useRef(new Map<string, { expiresAt: number; items: CatalogAnime[] }>());
  const catalogCooldownUntilRef = useRef(0);
  const availabilityGenerationRef = useRef(0);
  const catalogSearchGenerationRef = useRef(0);
  const youtubeSearchGenerationRef = useRef(0);
  const youtubeFeedGenerationRef = useRef(0);
  const youtubeRelatedGenerationRef = useRef(0);
  const youtubePlaybackGenerationRef = useRef(0);
  const animeOpenGenerationRef = useRef(0);
  const playbackGenerationRef = useRef(0);
  const [providerHealthPending, setProviderHealthPending] = useState<string | null>(null);
  const [theme, setTheme] = useState<AppTheme>(loadSavedTheme);
  const [appScale, setAppScale] = useState<AppScale>(loadSavedScale);
  const [appFont, setAppFont] = useState<AppFont>(loadSavedFont);
  const [autoSkip, setAutoSkip] = useState(loadSavedAutoSkip);
  const [showLoginModal, setShowLoginModal] = useState(false);

  useEffect(() => {
    window.history.replaceState({ anyWatchRoute: routeFromLocation() }, "", window.location.href);
    const handlePopState = () => {
      const nextRoute = routeFromLocation();
      const youtubeVideoId = new URLSearchParams(window.location.search).get("v")?.trim();
      if (nextRoute !== "youtube" || !youtubeVideoId) {
        youtubePlaybackGenerationRef.current += 1;
        setPlayer(null);
        setYoutubeWatchMode(false);
        if (!youtubeVideoId) setYoutubeSelection(null);
      }
      if (nextRoute !== "detail") {
        animeOpenGenerationRef.current += 1;
        setSelectedAnime(null);
        setEpisodes([]);
      }
      setRoute(nextRoute);
      setError(null);
    };
    window.addEventListener("popstate", handlePopState);
    void bootstrap();
    return () => window.removeEventListener("popstate", handlePopState);
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
    if (route !== "youtube") {
      youtubeSearchGenerationRef.current += 1;
      youtubeFeedGenerationRef.current += 1;
      youtubeRelatedGenerationRef.current += 1;
      youtubePlaybackGenerationRef.current += 1;
      return;
    }
    const cleanQuery = youtubeQuery.trim();
    if (cleanQuery.length < 2) {
      youtubeSearchGenerationRef.current += 1;
      setYoutubeResults([]);
      if (!youtubeWatchMode) setYoutubeSelection(null);
      void loadYoutubeFeed(youtubeTopic);
      return;
    }
    youtubeFeedGenerationRef.current += 1;
    const handle = window.setTimeout(() => void searchYoutube(cleanQuery), 320);
    return () => window.clearTimeout(handle);
  }, [youtubeQuery, route, youtubeTopic]);

  useEffect(() => {
    if (route !== "youtube") return;
    const videoId = new URLSearchParams(window.location.search).get("v")?.trim();
    const source = sources.find((item) => item.name === "Invidious" && item.status === "healthy");
    if (!videoId || !source || youtubeSelection?.id === videoId) return;
    let active = true;
    const placeholder: Anime = {
      id: videoId,
      provider: "Invidious",
      title: "YouTube video",
      coverUrl: `https://i.ytimg.com/vi/${encodeURIComponent(videoId)}/hqdefault.jpg`,
      bannerUrl: null,
      language: "YouTube",
      totalEpisodes: null,
      synopsis: null,
      isFavorite: myList.some((item) => item.animeId === animeKey("Invidious", videoId)),
    };
    setYoutubeSelection(placeholder);
    setYoutubeWatchMode(true);
    void loadYoutubeRelated(videoId);
    void api.getAnimeDetails("Invidious", videoId, placeholder.title)
      .then((details) => {
        if (active) setYoutubeSelection(mergeAnimeDetails(placeholder, detailPatch(details)));
      })
      .catch(() => undefined);
    return () => { active = false; };
  }, [route, sources, youtubeSelection?.id]);

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
      const [sourceList, history, favorites] = await Promise.all([
        api.listSources().catch(() => []),
        currentSession ? api.getContinueWatching(200).catch(() => []) : Promise.resolve([]),
        currentSession ? api.getMyList(300).catch(() => []) : Promise.resolve([]),
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
        if (catalog) {
          const images = [
            ...catalog.trending.flatMap((c) => [c.coverUrl, c.bannerUrl]),
            ...catalog.popularThisSeason.flatMap((c) => [c.coverUrl, c.bannerUrl]),
          ].filter(Boolean);
          preloadImages(images);
        }
      }).catch((err) => {
        console.warn("Discovery catalog error (fallback in use):", err);
      });
    } catch (err) {
      const appError = toAppError(err, "bootstrap");
      setError(appError);
      setAuthError(appError.message);
    } finally {
      setBootstrapping(false);
    }
  }

  async function refreshShelfData() {
    if (!session) {
      setContinueWatching([]);
      setMyList([]);
      return;
    }
    const [history, favorites] = await Promise.all([
      api.getContinueWatching(200).catch(() => []),
      api.getMyList(300).catch(() => []),
    ]);
    setContinueWatching(history);
    setMyList(favorites);
  }

  async function signIn(username: string, password: string) {
    setAuthError(null);
    setBootstrapping(true);
    try {
      await api.login(username, password);
      setShowLoginModal(false);
      await bootstrap();
      setShowLoginModal(false);
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
      window.history.replaceState({ anyWatchRoute: "home" }, "", "/");
      setRoute("home");
      setRouteStack([]);
      setSources([]);
      setSelectedSource(null);
      setContinueWatching([]);
      setMyList([]);
      setPlayer(null);
      setSelectedAnime(null);
      setEpisodes([]);
      setResults([]);
      setCatalogResults([]);
      setProviderResults([]);
      setCatalogSelection(null);
      setAvailability([]);
      setSearchSelection(null);
      setYoutubeResults([]);
      setYoutubeFeed([]);
      setYoutubeRelated([]);
      setYoutubeSelection(null);
      setYoutubeWatchMode(false);
      setDiscovery(null);
      detailCacheRef.current = {};
      availabilityCacheRef.current.clear();
      catalogSearchCacheRef.current.clear();
      animeOpenGenerationRef.current += 1;
      playbackGenerationRef.current += 1;
      youtubePlaybackGenerationRef.current += 1;
    }
  }

  function leaveYoutubeRoute() {
    youtubeSearchGenerationRef.current += 1;
    youtubeFeedGenerationRef.current += 1;
    youtubeRelatedGenerationRef.current += 1;
    youtubePlaybackGenerationRef.current += 1;
    if (!youtubeWatchMode) return;
    setPlayer(null);
    setYoutubeWatchMode(false);
    void refreshShelfData();
  }

  function navigate(nextRoute: Route) {
    if (nextRoute === route) return;
    if (route === "youtube" && nextRoute !== "youtube") leaveYoutubeRoute();
    setRouteStack((stack) => [...stack, route]);
    window.history.pushState({ anyWatchRoute: nextRoute }, "", pathForRoute(nextRoute));
    setRoute(nextRoute);
    setError(null);
  }

  function goBack() {
    const currentRoute = route;
    if (currentRoute === "youtube") leaveYoutubeRoute();
    setRouteStack((stack) => {
      const nextStack = [...stack];
      const previous = nextStack.pop();
      if (previous) {
        window.history.back();
      } else {
        window.history.replaceState({ anyWatchRoute: "home" }, "", "/");
        setRoute("home");
      }
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

  function withFavoriteState(items: Anime[]) {
    const favorites = new Set(myList.map((item) => item.animeId));
    return items.map((item) => ({
      ...item,
      isFavorite: favorites.has(animeKey(item.provider, item.id)),
    }));
  }

  async function searchYoutube(nextQuery = youtubeQuery) {
    const cleanQuery = nextQuery.trim();
    const generation = ++youtubeSearchGenerationRef.current;
    if (cleanQuery.length < 2) return;
    const source = sources.find((item) => item.languageGroup === "youtube" || item.name === "Invidious");
    if (!source || source.status === "unavailable") {
      setYoutubeResults([]);
      setYoutubeSelection(null);
      setError({
        code: "INVIDIOUS_UNAVAILABLE",
        message: "YouTube search is unavailable until an Invidious instance is configured and reachable.",
        operation: "youtube-search",
        retryable: true,
        correlationId: crypto.randomUUID(),
      });
      return;
    }

    setYoutubeLoading(true);
    setError(null);
    try {
      const items = withFavoriteState(await api.searchSource(source.name, cleanQuery));
      if (generation !== youtubeSearchGenerationRef.current) return;
      setYoutubeResults(items);
      setYoutubeSelection((current) => {
        if (current && items.some((item) => item.id === current.id)) return current;
        return items[0] ?? null;
      });
    } catch (err) {
      if (generation !== youtubeSearchGenerationRef.current) return;
      const appError = toAppError(err, "youtube-search");
      if (providerFailureMakesOffline(appError)) markProviderOffline(source.name, appError.code);
      setYoutubeResults([]);
      setYoutubeSelection(null);
      setError(appError);
    } finally {
      if (generation === youtubeSearchGenerationRef.current) setYoutubeLoading(false);
    }
  }

  async function loadAllYoutubeCatalogs() {
    const source = sources.find((item) => item.name === "Invidious" || item.languageGroup === "youtube");
    if (!source || source.status === "unavailable") return;
    const providerName = source.name;

    const sections: Array<"trending" | "music" | "films" | "anime" | "gaming" | "news"> = [
      "trending",
      "music",
      "films",
      "anime",
      "gaming",
      "news",
    ];

    // Pre-populate immediately from 24h cache
    for (const key of sections) {
      const cached = loadCachedYouTubeCatalog(key);
      if (cached && cached.items.length > 0) {
        const enriched = withFavoriteState(cached.items);
        setYoutubeCatalogs((prev) => ({
          ...prev,
          [key]: enriched,
        }));
        if (key === "trending" && youtubeTopic === "All") {
          setYoutubeFeed(enriched);
        }
      }
    }

    const fetchSection = async (
      key: "trending" | "music" | "films" | "anime" | "gaming" | "news",
      fetcher: () => Promise<Anime[]>,
    ) => {
      const cached = loadCachedYouTubeCatalog(key);
      if (!cached || !cached.isFresh) {
        setYoutubeCatalogs((prev) => ({
          ...prev,
          loading: { ...prev.loading, [key]: true },
          errors: { ...prev.errors, [key]: null },
        }));
      }
      try {
        const rawItems = await fetcher();
        const items = withFavoriteState(rawItems);
        saveCachedYouTubeCatalog(key, items);
        setYoutubeCatalogs((prev) => ({
          ...prev,
          [key]: items,
          loading: { ...prev.loading, [key]: false },
        }));
      } catch (err) {
        const appErr = toAppError(err, `youtube-catalog-${key}`);
        setYoutubeCatalogs((prev) => ({
          ...prev,
          errors: { ...prev.errors, [key]: appErr.message },
          loading: { ...prev.loading, [key]: false },
        }));
      }
    };

    void fetchSection("trending", async () => {
      try {
        const items = await api.getYouTubeTrending();
        if (items && items.length) {
          if (youtubeTopic === "All") setYoutubeFeed(withFavoriteState(items));
          return items;
        }
        const pop = await api.getYouTubePopular();
        if (youtubeTopic === "All") setYoutubeFeed(withFavoriteState(pop));
        return pop;
      } catch {
        const pop = await api.getYouTubePopular();
        if (youtubeTopic === "All") setYoutubeFeed(withFavoriteState(pop));
        return pop;
      }
    });

    void fetchSection("music", async () => {
      try {
        const items = await api.getYouTubeTrending("Music");
        if (items && items.length) return items;
        return api.searchSource(providerName, "trending music official audio top hits");
      } catch {
        return api.searchSource(providerName, "trending music official audio top hits");
      }
    });

    void fetchSection("films", async () => {
      try {
        const items = await api.getYouTubeTrending("Movies");
        if (items && items.length) return items;
        return api.searchSource(providerName, "official movie trailer short film cinema");
      } catch {
        return api.searchSource(providerName, "official movie trailer short film cinema");
      }
    });

    void fetchSection("anime", async () => {
      return api.searchSource(providerName, "anime animation opening ending trailer short");
    });

    void fetchSection("gaming", async () => {
      try {
        const items = await api.getYouTubeTrending("Gaming");
        if (items && items.length) return items;
        return api.searchSource(providerName, "trending gaming gameplay walkthrough");
      } catch {
        return api.searchSource(providerName, "trending gaming gameplay walkthrough");
      }
    });

    void fetchSection("news", async () => {
      try {
        const items = await api.getYouTubeTrending("News");
        if (items && items.length) return items;
        return api.searchSource(providerName, "breaking news world tech latest");
      } catch {
        return api.searchSource(providerName, "breaking news world tech latest");
      }
    });
  }

  async function loadYoutubeFeed(topic: YouTubeTopic = youtubeTopic) {
    const generation = ++youtubeFeedGenerationRef.current;
    const source = sources.find((item) => item.languageGroup === "youtube" || item.name === "Invidious");
    if (!source || source.status === "unavailable") {
      setYoutubeFeed([]);
      setYoutubeFeedError(null);
      return;
    }

    const topicKey = topic.toLowerCase();
    const cached = loadCachedYouTubeCatalog(topicKey);
    if (cached && cached.items.length > 0) {
      setYoutubeFeed(withFavoriteState(cached.items));
      setYoutubeFeedError(null);
    }

    if (topic === "All") {
      setYoutubeFeedLoading(!cached);
      setYoutubeFeedError(null);
      void loadAllYoutubeCatalogs();
      try {
        let items: Anime[];
        try {
          items = await api.getYouTubeTrending();
          if (!items.length) items = await api.getYouTubePopular();
        } catch {
          items = await api.getYouTubePopular();
        }
        if (generation !== youtubeFeedGenerationRef.current) return;
        const enriched = withFavoriteState(items);
        saveCachedYouTubeCatalog("all", enriched);
        saveCachedYouTubeCatalog("trending", enriched);
        setYoutubeFeed(enriched);
      } catch (err) {
        if (generation !== youtubeFeedGenerationRef.current) return;
        if (!cached) {
          const appError = toAppError(err, "youtube-feed");
          setYoutubeFeed([]);
          setYoutubeFeedError(appError.message);
        }
      } finally {
        if (generation === youtubeFeedGenerationRef.current) setYoutubeFeedLoading(false);
      }
      return;
    }

    if (!cached) setYoutubeFeed([]);
    setYoutubeFeedError(null);
    setYoutubeFeedLoading(!cached);
    try {
      let items: Anime[];
      if (topic === "Trending") {
        try {
          items = await api.getYouTubeTrending();
          if (!items.length) items = await api.getYouTubePopular();
        } catch {
          items = await api.getYouTubePopular();
        }
      } else if (topic === "Music") {
        items = await api.getYouTubeTrending("Music");
      } else if (topic === "Gaming") {
        items = await api.getYouTubeTrending("Gaming");
      } else if (topic === "News") {
        items = await api.getYouTubeTrending("News");
      } else if (topic === "Films") {
        try {
          items = await api.getYouTubeTrending("Movies");
          if (!items.length) items = await api.searchSource(source.name, "official movie trailer short film cinema");
        } catch {
          items = await api.searchSource(source.name, "official movie trailer short film cinema");
        }
      } else if (topic === "Anime") {
        items = await api.searchSource(source.name, "anime animation opening ending trailer short");
      } else {
        items = await api.getYouTubeTrending();
      }
      if (generation !== youtubeFeedGenerationRef.current) return;
      const enriched = withFavoriteState(items);
      saveCachedYouTubeCatalog(topicKey, enriched);
      setYoutubeFeed(enriched);
      const catKey = topic.toLowerCase() as keyof typeof youtubeCatalogs;
      if (catKey) {
        setYoutubeCatalogs((prev) => ({ ...prev, [catKey]: enriched }));
      }
    } catch (err) {
      if (generation !== youtubeFeedGenerationRef.current) return;
      if (!cached) {
        const appError = toAppError(err, "youtube-feed");
        setYoutubeFeed([]);
        setYoutubeFeedError(appError.message);
      }
    } finally {
      if (generation === youtubeFeedGenerationRef.current) setYoutubeFeedLoading(false);
    }
  }

  async function loadYoutubeRelated(videoId: string) {
    const generation = ++youtubeRelatedGenerationRef.current;
    setYoutubeRelated([]);
    setYoutubeRelatedError(null);
    setYoutubeRelatedLoading(true);

    try {
      const related = withFavoriteState(await api.getYouTubeRelated(videoId));
      if (generation !== youtubeRelatedGenerationRef.current) return;
      setYoutubeRelated(related);
    } catch (err) {
      if (generation !== youtubeRelatedGenerationRef.current) return;
      const appError = toAppError(err, "youtube-related");
      setYoutubeRelated([]);
      setYoutubeRelatedError(appError.message);
    } finally {
      if (generation === youtubeRelatedGenerationRef.current) setYoutubeRelatedLoading(false);
    }
  }

  function selectYoutubeVideo(video: Anime, openWatchRoom = true) {
    if (video.provider === "YouTube") {
      video = { ...video, provider: "Invidious" };
    }
    setYoutubeSelection(video);
    if (openWatchRoom) {
      const url = `/youtube?v=${encodeURIComponent(video.id)}`;
      if (`${window.location.pathname}${window.location.search}` !== url) {
        window.history.pushState({ anyWatchRoute: "youtube", videoId: video.id }, "", url);
      }
      setYoutubeWatchMode(true);
      void loadYoutubeRelated(video.id);
      void api.getAnimeDetails(video.provider, video.id, video.title).then((details) => {
        setYoutubeSelection((curr) => curr && curr.id === video.id ? mergeAnimeDetails(curr, detailPatch(details)) : curr);
      }).catch(() => undefined);
    }
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
      const providerOutcome = activeSource
        ? await searchProviderResults(cleanQuery, activeSource)
            .then((items) => ({ ok: true as const, items }))
            .catch((err) => ({ ok: false as const, error: toAppError(err, "provider-search") }))
        : { ok: true as const, items: [] };
      if (generation !== catalogSearchGenerationRef.current) return;
      const directItems = providerOutcome.ok ? providerOutcome.items : [];
      setProviderResults(directItems);
      setCatalogResults([]);
      setCatalogSelection(null);
      if (!providerOutcome.ok) {
        setError(providerOutcome.error);
        if (activeSource && providerOutcome.error.code === "PROVIDER_CAPTCHA") {
          const blocked = { ...activeSource, status: "unavailable", failureCode: providerOutcome.error.code };
          setSources((current) => current.map((source) => source.name === blocked.name ? blocked : source));
          setSelectedSource(blocked);
        }
      }
      if (directItems.length) {
        setSearchSelection(directItems[0]);
      } else {
        setSearchSelection(null);
      }
    } finally {
      if (generation === catalogSearchGenerationRef.current) setLoading(false);
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
    setCatalogSelection(null);
    setAvailability([]);
    setSelectedSource(sources.find((source) => source.name === anime.provider) ?? null);
    setSearchSelection(anime);
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
        : await api.resolveAvailability(
            catalog.catalogId,
            catalog.title,
            [catalog.nativeTitle ?? "", ...(catalog.synonyms ?? [])],
            group,
          );
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
    const inMemory = detailCacheRef.current[key];
    if (inMemory && Object.keys(inMemory).length > 0) {
      return mergeAnimeDetails(anime, inMemory);
    }
    const cachedDetails = getCachedAnimeDetails(anime.provider, anime.id);
    if (cachedDetails && Object.keys(cachedDetails).length > 0) {
      detailCacheRef.current[key] = cachedDetails;
      if (Object.keys(cachedDetails).length) mergeAnimeEverywhere(key, cachedDetails);
      return mergeAnimeDetails(anime, cachedDetails);
    }

    try {
      const details = await api.getAnimeDetails(anime.provider, anime.id, anime.title);
      const patch = detailPatch(details);
      detailCacheRef.current[key] = patch;
      saveCachedAnimeDetails(anime.provider, anime.id, patch);
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
    setYoutubeResults((items) =>
      items.map((item) => (animeKey(item.provider, item.id) === key ? mergeAnimeDetails(item, patch) : item)),
    );
    setYoutubeSelection((anime) =>
      anime && animeKey(anime.provider, anime.id) === key ? mergeAnimeDetails(anime, patch) : anime,
    );
  }

  async function openAnime(anime: Anime) {
    const generation = ++animeOpenGenerationRef.current;
    playbackGenerationRef.current += 1;
    if (anime.provider === "YouTube") {
      anime = { ...anime, provider: "Invidious" };
    }
    if (anime.provider === "Invidious") {
      navigate("youtube");
      selectYoutubeVideo(anime, true);
      return;
    }
    setSelectedAnime(anime);

    // Fast instant display from episode cache
    const cachedEps = getCachedEpisodes(anime.provider, anime.id);
    if (cachedEps && cachedEps.length > 0) {
      setEpisodes(cachedEps);
      setLoadingEpisodes(false);
    } else {
      setEpisodes([]);
      setLoadingEpisodes(true);
    }
    setError(null);
    if (route !== "detail") navigate("detail");
    const linkedAnime = await linkCatalogAnime(anime);
    if (generation !== animeOpenGenerationRef.current) return;
    if (linkedAnime !== anime) setSelectedAnime(linkedAnime);
    void enrichAnime(linkedAnime);
    try {
      const nextEpisodes = await api.getEpisodes(linkedAnime.provider, linkedAnime.id);
      if (generation !== animeOpenGenerationRef.current) return;
      setEpisodes(nextEpisodes);
      saveCachedEpisodes(linkedAnime.provider, linkedAnime.id, nextEpisodes);
    } catch (err) {
      if (generation !== animeOpenGenerationRef.current) return;
      if (!cachedEps || cachedEps.length === 0) {
        const appError = toAppError(err, "episodes");
        if (providerFailureMakesOffline(appError)) markProviderOffline(anime.provider, appError.code);
        setError(appError);
      }
    } finally {
      if (generation === animeOpenGenerationRef.current) setLoadingEpisodes(false);
    }
  }

  async function linkCatalogAnime(anime: Anime): Promise<Anime> {
    if (anime.catalogId) return anime;
    try {
      const items = await api.searchCatalog(anime.title);
      const match = exactCatalogMatch(items, anime);
      return match ? catalogToAnime(match, anime) : anime;
    } catch {
      return anime;
    }
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
    setYoutubeResults((items) =>
      items.map((item) => animeKey(item.provider, item.id) === key ? { ...item, isFavorite } : item),
    );
    setYoutubeFeed((items) =>
      items.map((item) => animeKey(item.provider, item.id) === key ? { ...item, isFavorite } : item),
    );
    setYoutubeRelated((items) =>
      items.map((item) => animeKey(item.provider, item.id) === key ? { ...item, isFavorite } : item),
    );
    setYoutubeSelection((anime) =>
      anime && animeKey(anime.provider, anime.id) === key ? { ...anime, isFavorite } : anime,
    );
  }

  async function playEpisode(anime: Anime, episode: Episode, startTime = 0, episodeList = episodes) {
    const generation = ++playbackGenerationRef.current;
    setError(null);
    try {
      const playback = await api.preparePlayback(anime.provider, episode.id);
      if (generation !== playbackGenerationRef.current) return;
      setPlayer({ anime, episode, episodes: episodeList, playback, startTime });
    } catch (err) {
      if (generation !== playbackGenerationRef.current) return;
      const appError = toAppError(err, "playback");
      if (providerFailureMakesOffline(appError)) markProviderOffline(anime.provider, appError.code);
      setError(appError);
    }
  }

  async function playYoutubeVideo(anime: Anime, forceStream = false) {
    if (anime.provider === "YouTube") {
      anime = { ...anime, provider: "Invidious" };
    }
    const watchUrl = `/youtube?v=${encodeURIComponent(anime.id)}`;
    if (`${window.location.pathname}${window.location.search}` !== watchUrl) {
      window.history.pushState({ anyWatchRoute: "youtube", videoId: anime.id }, "", watchUrl);
    }
    const generation = ++youtubePlaybackGenerationRef.current;
    setPlayer(null);
    setYoutubeLoading(true);
    setError(null);
    try {
      const cached = getCachedYouTubeMetadata(anime.id);
      if (cached) {
        anime = {
          ...anime,
          title: cached.title || anime.title,
          coverUrl: cached.coverUrl || anime.coverUrl,
          synopsis: cached.synopsis || anime.synopsis,
        };
      }
      setYoutubeSelection(anime);
      setYoutubeWatchMode(true);
      void loadYoutubeRelated(anime.id);

      saveCachedYouTubeMetadata({
        id: anime.id,
        provider: anime.provider,
        title: anime.title,
        coverUrl: anime.coverUrl,
        bannerUrl: anime.bannerUrl,
        synopsis: anime.synopsis,
      });

      const episodeId = anime.id;
      const history = findHistoryForAnime(anime, continueWatching);
      const [playback, details] = await Promise.all([
        api.preparePlayback(anime.provider, episodeId).catch(() => null),
        api.getAnimeDetails(anime.provider, anime.id, anime.title).catch(() => null),
      ]);
      if (generation !== youtubePlaybackGenerationRef.current) return;

      const enriched = details ? mergeAnimeDetails(anime, detailPatch(details)) : anime;
      setYoutubeSelection(enriched);

      const videoEpisode: Episode = {
        id: episodeId,
        number: 1,
        title: enriched.title,
        thumbnail: enriched.coverUrl,
      };

      if (playback && !forceStream) {
        setYoutubeEmbedPlaying(false);
        setPlayer({
          anime: enriched,
          episode: videoEpisode,
          episodes: [videoEpisode],
          playback,
          startTime: history?.positionSeconds ?? 0,
        });
      } else {
        setPlayer(null);
        setYoutubeEmbedPlaying(true);
      }
    } catch (err) {
      if (generation !== youtubePlaybackGenerationRef.current) return;
      setPlayer(null);
      setYoutubeEmbedPlaying(true);
      setError(null);
    } finally {
      if (generation === youtubePlaybackGenerationRef.current) setYoutubeLoading(false);
    }
  }

  function playTorrentFilm(task: TorrentTask, metadata?: MediaMetadata | null) {
    if (task.status.type !== "ready") return;
    const info = parseFilmReleaseInfo(task.title);
    const title = info.cleanTitle || task.title;
    const coverUrl = metadata?.coverUrl || "";
    const bannerUrl = metadata?.bannerUrl || null;
    const subtitles = task.status.data.subtitles.map((s) => ({
      language: s.language,
      url: api.getTorrentSubtitleUrl(task.id, s.language_code),
    }));
    const anime: Anime = {
      id: `torrent:${task.id}`,
      provider: "Shared Storage",
      title,
      coverUrl,
      bannerUrl,
      language: "Torrent",
      totalEpisodes: 1,
      synopsis: metadata?.description || null,
      isFavorite: false,
    };
    const episode: Episode = {
      id: `torrent-ep:${task.id}`,
      number: 1,
      title: info.cleanTitle,
      thumbnail: coverUrl,
    };
    const playback: Playback = {
      sessionId: `torrent-${task.id}`,
      playbackUrl: api.getTorrentStreamUrl(task.id),
      streamKind: "native",
      subtitles,
      qualities: [info.qualityBadge || "HD"],
    };
    setPlayer({
      anime,
      episode,
      episodes: [episode],
      playback,
      startTime: 0,
    });
  }

  function handleRemoveContinueWatching(animeId: string) {
    setContinueWatching((current) => current.filter((item) => item.animeId !== animeId));
    if (session) {
      void api.removeContinueWatching(animeId).catch(() => undefined);
    }
  }

  function handleClearContinueWatching() {
    setContinueWatching((current) => current.filter((item) => item.provider !== "Invidious" && item.provider !== "YouTube"));
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
    if (!session) {
      setShowLoginModal(true);
      return;
    }
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
  const providerFallbackCatalog = useMemo(() => {
    const seen = new Set<number>();
    return [...(discovery?.trending ?? []), ...(discovery?.popularThisSeason ?? [])]
      .filter((item) => {
        if (seen.has(item.catalogId)) return false;
        seen.add(item.catalogId);
        return true;
      })
      .slice(0, 12);
  }, [discovery]);
  const latestHistory = continueWatching[0] ?? null;
  const featuredAnime = latestHistory ? historyToAnime(latestHistory, myList) : savedAnime[0] ?? null;
  const heroImage =
    selectedAnime?.bannerUrl ||
    selectedAnime?.coverUrl ||
    youtubeSelection?.bannerUrl ||
    youtubeSelection?.coverUrl ||
    searchSelection?.bannerUrl ||
    searchSelection?.coverUrl ||
    featuredAnime?.bannerUrl ||
    featuredAnime?.coverUrl;
  const selectedAnimeIsFavorite = selectedAnime
    ? selectedAnime.isFavorite || myList.some((item) => item.animeId === animeKey(selectedAnime.provider, selectedAnime.id))
    : false;
  const resumeHistory = selectedAnime ? findHistoryForAnime(selectedAnime, continueWatching) : undefined;
  const youtubeSource = sources.find((source) => source.languageGroup === "youtube") ?? null;
  const youtubeResume = youtubeSelection ? findHistoryForAnime(youtubeSelection, continueWatching) : undefined;
  const youtubeSelectionIsFavorite = youtubeSelection
    ? youtubeSelection.isFavorite || myList.some((item) => item.animeId === animeKey(youtubeSelection.provider, youtubeSelection.id))
    : false;
  const youtubePlayerContext = route === "youtube" && youtubeWatchMode && youtubeSelection && player
    && animeKey(player.anime.provider, player.anime.id) === animeKey(youtubeSelection.provider, youtubeSelection.id)
    ? player
    : null;

  if (bootstrapping) {
    return <BootSplash />;
  }

  if (!session) {
    return (
      <LoginScreen
        error={authError ?? error?.message ?? null}
        onLogin={async (username, password) => {
          await signIn(username, password);
        }}
      />
    );
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
                onShowDownloads={() => navigate("download")}
                onShowSettings={() => navigate("settings")}
                session={session ?? null}
                onShowAdmin={session?.role === "admin" ? () => navigate("admin") : undefined}
                onSignOut={session ? () => void signOut() : undefined}
                onShowSignIn={() => setShowLoginModal(true)}
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

          {route === "admin" && session && session.role === "admin" && (
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
              suggestedCatalog={providerFallbackCatalog}
              onQueryChange={(q) => {
                setQuery(q);
                if (q.trim().length >= 2) void searchCatalog(q);
              }}
              onSearch={(targetQuery) => void searchCatalog(targetQuery ?? query)}
              onLanguageChange={selectSearchLanguage}
              onProviderSelect={(option) => void selectCatalogProvider(option)}
              onProviderSourceSelect={selectProviderSource}
              onProviderHealthRetry={(source) => void retryProviderHealth(source)}
              providerHealthPending={providerHealthPending}
              onSelectProviderResult={selectProviderResult}
              onSelectCatalog={selectCatalogResult}
              onOpenCatalog={openCatalogSearch}
              onOpenAnime={(anime) => void openAnime(anime)}
              onDownload={(anime) => void openAnime(anime)}
              onToggleMyList={(anime) => void toggleMyList(anime)}
              onBack={goBack}
              myList={myList}
            />
          )}

          {route === "youtube" && (
            <YouTubePage
              key="youtube"
              query={youtubeQuery}
              results={youtubeResults}
              selectedVideo={youtubeSelection}
              source={youtubeSource}
              loading={youtubeLoading}
              topic={youtubeTopic}
              feedVideos={youtubeFeed}
              feedLoading={youtubeFeedLoading}
              feedError={youtubeFeedError}
              catalogs={youtubeCatalogs}
              viewMode={youtubeViewMode}
              onViewModeChange={setYoutubeViewMode}
              relatedVideos={youtubeRelated}
              relatedLoading={youtubeRelatedLoading}
              relatedError={youtubeRelatedError}
              continueWatching={continueWatching}
              onRemoveContinueWatching={handleRemoveContinueWatching}
              onClearContinueWatching={handleClearContinueWatching}
              watchMode={youtubeWatchMode}
              embedPlaying={youtubeEmbedPlaying}
              resume={youtubeResume}
              isFavorite={youtubeSelectionIsFavorite}
              playerContext={youtubePlayerContext}
              autoSkip={autoSkip}
              onAutoSkipChange={setAutoSkip}
              onPlayPlayerEpisode={(episode) => youtubePlayerContext
                ? playEpisode(youtubePlayerContext.anime, episode, 0, youtubePlayerContext.episodes)
                : Promise.resolve()}
              onPlayNextVideo={(next) => void playYoutubeVideo(next)}
              onClosePlayer={() => {
                setPlayer(null);
                void refreshShelfData();
              }}
              onQueryChange={(q) => {
                youtubeSearchGenerationRef.current += 1;
                setYoutubeQuery(q);
                if (youtubeWatchMode) {
                  youtubeRelatedGenerationRef.current += 1;
                  youtubePlaybackGenerationRef.current += 1;
                  setPlayer(null);
                  setYoutubeWatchMode(false);
                  window.history.replaceState({ anyWatchRoute: "youtube" }, "", "/youtube");
                }
              }}
              onSearch={() => void searchYoutube()}
              onTopicChange={(t, mode) => {
                youtubeRelatedGenerationRef.current += 1;
                youtubePlaybackGenerationRef.current += 1;
                setPlayer(null);
                setYoutubeTopic(t);
                if (mode) setYoutubeViewMode(mode);
                setYoutubeQuery("");
                setYoutubeWatchMode(false);
                window.history.replaceState({ anyWatchRoute: "youtube" }, "", "/youtube");
              }}
              onRetryFeed={() => void loadYoutubeFeed(youtubeTopic)}
              onRetryRelated={() => youtubeSelection && void loadYoutubeRelated(youtubeSelection.id)}
              onSelect={(video) => selectYoutubeVideo(video, true)}
              onPlay={(video, forceStream) => void playYoutubeVideo(video, forceStream)}
              onCloseWatch={() => {
                youtubeRelatedGenerationRef.current += 1;
                youtubePlaybackGenerationRef.current += 1;
                setPlayer(null);
                setYoutubeWatchMode(false);
                window.history.replaceState({ anyWatchRoute: "youtube" }, "", "/youtube");
                void refreshShelfData();
              }}
              onToggleMyList={(video) => void toggleMyList(video)}
              onBack={goBack}
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

          {route === "download" && (
            <DownloadsPage
              key="download"
              onBack={goBack}
              session={session}
              onShowSignIn={() => setShowLoginModal(true)}
              onPlayFilm={playTorrentFilm}
            />
          )}

          {route === "donate" && (
            <DonatePage
              key="donate"
              onBack={goBack}
            />
          )}
        </AnimatePresence>
        </LayoutGroup>
      </main>

      <AnimatePresence>
        {player && !youtubePlayerContext && (
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

      <AnimatePresence>
        {showLoginModal && (
          <div className="login-modal-overlay" onClick={() => setShowLoginModal(false)}>
            <div className="login-modal-dialog" onClick={(e) => e.stopPropagation()}>
              <LoginScreen
                error={authError ?? error?.message ?? null}
                onLogin={async (username, password) => {
                  await signIn(username, password);
                }}
                onClose={() => setShowLoginModal(false)}
              />
            </div>
          </div>
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
    { route: "download", label: "Film Requests", icon: <Film size={20} /> },
    { route: "youtube", label: "YouTube", icon: <span className="youtube-nav-mark"><Play size={15} fill="currentColor" /></span> },
    { route: "donate", label: "Donate", icon: <Heart size={20} /> },
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
            data-route={item.route}
            className={route === item.route ? "active" : ""}
            aria-label={item.label}
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

function formatSpeed(bytesPerSec: number): string {
  if (bytesPerSec >= 1024 * 1024) {
    return `${(bytesPerSec / (1024 * 1024)).toFixed(1)} MB/s`;
  }
  return `${(bytesPerSec / 1024).toFixed(0)} KB/s`;
}

function formatTorrentBytes(bytes: number): string {
  if (bytes >= 1024 * 1024 * 1024) {
    return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }
  return `${(bytes / 1024).toFixed(0)} KB`;
}

function formatEta(seconds: number): string {
  if (seconds <= 0) return "0s";
  if (seconds >= 3600) {
    const hours = Math.floor(seconds / 3600);
    const mins = Math.floor((seconds % 3600) / 60);
    return `${hours}h ${mins}m`;
  }
  if (seconds >= 60) {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins}m ${secs}s`;
  }
  return `${seconds}s`;
}

function DonatePage({
  onBack,
}: {
  onBack?: () => void;
}) {
  const shouldReduceMotion = useReducedMotion();
  const [copied, setCopied] = useState(false);

  const accountNumber = "31658367";
  const bankName = "ACB (Ngân hàng TMCP Á Châu)";
  const accountHolder = "DANG NGUYEN THIEN PHUC";
  const memoText = "Donate any-watch";

  const handleCopy = () => {
    void navigator.clipboard.writeText(accountNumber);
    setCopied(true);
    setTimeout(() => setCopied(false), 2500);
  };

  return (
    <motion.section
      className="page-stage stage-donate"
      initial="hidden"
      animate="show"
      exit="hidden"
      variants={shouldReduceMotion ? { hidden: { opacity: 0 }, show: { opacity: 1 } } : fadeUpVariant}
    >
      <div className="page-stage-header">
        {onBack && (
          <button
            type="button"
            className="stage-back-button"
            aria-label="Back"
            onClick={onBack}
          >
            <ArrowLeft size={18} />
          </button>
        )}
        <div className="page-stage-title-group">
          <h1>Support any-watch</h1>
          <p>
            any-watch is a non-profit personal theatre project. Your support helps maintain the homelab server, storage drives, and streaming bandwidth.
          </p>
        </div>
      </div>

      <div className="donate-container">
        <div className="donate-card">
          <div className="donate-qr-wrapper">
            <img
              src="/donate-qr.png"
              alt="VietQR ACB 31658367 DANG NGUYEN THIEN PHUC"
              className="donate-qr-image"
              onError={useLogoFallback}
            />
          </div>

          <div className="donate-info">
            <div className="donate-badge">
              <Heart size={16} className="donate-heart-icon" />
              <span>VietQR / ACB Instant Transfer</span>
            </div>

            <div className="donate-field">
              <span className="donate-label">Bank:</span>
              <strong className="donate-value">{bankName}</strong>
            </div>

            <div className="donate-field">
              <span className="donate-label">Account Number:</span>
              <div className="donate-copy-row">
                <strong className="donate-value highlight">{accountNumber}</strong>
                <button
                  type="button"
                  className="donate-copy-btn"
                  onClick={handleCopy}
                  aria-label={copied ? "Copied" : "Copy Account Number"}
                >
                  {copied ? <Check size={16} /> : <Copy size={16} />}
                  <span>{copied ? "Copied!" : "Copy"}</span>
                </button>
              </div>
            </div>

            <div className="donate-field">
              <span className="donate-label">Beneficiary:</span>
              <strong className="donate-value">{accountHolder}</strong>
            </div>

            <div className="donate-field">
              <span className="donate-label">Transfer Memo:</span>
              <span className="donate-value">{memoText}</span>
            </div>

            <p className="donate-thankyou">
              Thank you for keeping any-watch fast, reliable, and ad-free! ❤️
            </p>
          </div>
        </div>
      </div>
    </motion.section>
  );
}

export interface FilmBadge {
  label: string;
  category: "quality" | "source" | "hdr" | "audio" | "sub" | "codec" | "group";
}

interface FilmReleaseParsed {
  cleanTitle: string;
  originalTitle: string;
  year?: string;
  sourceType?: string;
  quality?: string;
  qualityBadge?: string;
  codec?: string;
  hdr?: string;
  audio?: string;
  episodeInfo?: string;
  releaseGroup?: string;
  isVietSub: boolean;
  isThuyetMinh: boolean;
  isEngSub: boolean;
  isMultiSub: boolean;
  tags: string[];
  richBadges: FilmBadge[];
}

function parseFilmReleaseInfo(rawTitle: string): FilmReleaseParsed {
  const lower = rawTitle.toLowerCase();

  let releaseGroup: string | undefined;
  const groupMatchStart = rawTitle.match(/^\[([a-zA-Z0-9_ .–-]{2,24})\]/);
  if (groupMatchStart) {
    releaseGroup = groupMatchStart[1].trim();
  } else {
    const groupMatchEnd = rawTitle.match(/-([a-zA-Z0-9]{2,15})(?:\.[a-zA-Z0-9]{2,4})?$/);
    if (groupMatchEnd) {
      releaseGroup = groupMatchEnd[1].trim();
    }
  }

  const yearMatch = rawTitle.match(/\b(19\d\d|20\d\d)\b/);
  const year = yearMatch ? yearMatch[1] : undefined;

  let episodeInfo: string | undefined;
  const sxxExxMatch = rawTitle.match(/\bS(\d{1,2})E(\d{1,3})\b/i);
  if (sxxExxMatch) {
    episodeInfo = `S${parseInt(sxxExxMatch[1], 10)} E${parseInt(sxxExxMatch[2], 10)}`;
  } else {
    const epMatch = rawTitle.match(/\b(?:Episode|Ep\.?|E)\s*(\d{1,4})\b/i) || rawTitle.match(/-\s*(\d{1,3})\s*(?:\(|\[|\b)/);
    if (epMatch) {
      episodeInfo = `Ep ${parseInt(epMatch[1], 10)}`;
    } else {
      const seasonMatch = rawTitle.match(/\b(?:Season|S)\s*(\d{1,2})\b/i);
      if (seasonMatch) {
        episodeInfo = `Season ${parseInt(seasonMatch[1], 10)}`;
      } else if (/\b(batch|complete|all episodes)\b/i.test(rawTitle)) {
        episodeInfo = "Batch";
      } else if (/\b(movie|film)\b/i.test(rawTitle)) {
        episodeInfo = "Movie";
      }
    }
  }

  // Source rip type
  let sourceType: string | undefined;
  if (/\b(remux|bdremux)\b/i.test(rawTitle)) {
    sourceType = "Remux";
  } else if (/\b(bluray|bdrip|brrip|blu-ray)\b/i.test(rawTitle)) {
    sourceType = "BluRay";
  } else if (/\b(web-dl|webdl|web-rip|webrip)\b/i.test(rawTitle)) {
    sourceType = "WEB-DL";
  } else if (/\b(imax|imax enhanced)\b/i.test(rawTitle)) {
    sourceType = "IMAX";
  } else if (/\b(hdtv|pdtv|dsr)\b/i.test(rawTitle)) {
    sourceType = "HDTV";
  }

  let quality: string | undefined;
  let qualityBadge = "HD";
  if (lower.includes("2160p") || lower.includes("4k") || lower.includes("uhd")) {
    quality = "4K";
    qualityBadge = "4K UHD";
  } else if (lower.includes("1080p") || lower.includes("fhd") || lower.includes("1080i")) {
    quality = "1080p";
    qualityBadge = "1080p FHD";
  } else if (lower.includes("720p") || lower.includes("hd")) {
    quality = "720p";
    qualityBadge = "720p HD";
  } else if (lower.includes("480p") || lower.includes("sd") || lower.includes("576p")) {
    quality = "480p";
    qualityBadge = "480p SD";
  }

  let codec: string | undefined;
  if (lower.includes("hevc") || lower.includes("x265") || lower.includes("h.265") || lower.includes("h265")) {
    codec = lower.includes("10bit") || lower.includes("10-bit") || lower.includes("hi10p") ? "HEVC 10-bit" : "HEVC";
  } else if (lower.includes("x264") || lower.includes("h.264") || lower.includes("h264") || lower.includes("avc")) {
    codec = lower.includes("10bit") || lower.includes("10-bit") ? "x264 10-bit" : "x264";
  } else if (lower.includes("av1")) {
    codec = "AV1";
  }

  let hdr: string | undefined;
  if (lower.includes("dolby vision") || lower.includes("dovi") || lower.includes(".dv.")) {
    hdr = "Dolby Vision";
  } else if (lower.includes("hdr10+") || lower.includes("hdr10plus")) {
    hdr = "HDR10+";
  } else if (lower.includes("hdr10") || lower.includes("hdr")) {
    hdr = "HDR10";
  }

  let audio: string | undefined;
  if (lower.includes("atmos")) {
    audio = "Dolby Atmos";
  } else if (lower.includes("truehd 7.1") || lower.includes("truehd7.1")) {
    audio = "TrueHD 7.1";
  } else if (lower.includes("truehd")) {
    audio = "TrueHD";
  } else if (lower.includes("dts-hd ma") || lower.includes("dts-hdma") || lower.includes("dts-ma")) {
    audio = "DTS-HD MA";
  } else if (lower.includes("dts:x") || lower.includes("dts-x")) {
    audio = "DTS:X";
  } else if (lower.includes("dts-hd") || lower.includes("dts")) {
    audio = "DTS-HD";
  } else if (lower.includes("ddp5.1") || lower.includes("ddp 5.1") || lower.includes("eac3") || lower.includes("dd+")) {
    audio = "DDP 5.1";
  } else if (lower.includes("ac3 5.1") || lower.includes("dd5.1") || lower.includes("dd 5.1")) {
    audio = "DD 5.1";
  } else if (lower.includes("aac5.1") || lower.includes("aac 5.1")) {
    audio = "AAC 5.1";
  } else if (lower.includes("flac")) {
    audio = "FLAC";
  } else if (lower.includes("dual audio") || lower.includes("dual-audio")) {
    audio = "Dual Audio";
  } else if (lower.includes("multi audio") || lower.includes("multi-audio")) {
    audio = "Multi Audio";
  }

  const isVietSub =
    lower.includes("vietsub") ||
    lower.includes("viet sub") ||
    lower.includes("vietnamese") ||
    lower.includes("phụ đề việt");

  const isThuyetMinh =
    lower.includes("thuyết minh") ||
    lower.includes("thuyet minh") ||
    lower.includes("lồng tiếng") ||
    lower.includes("long tieng");

  const isMultiSub =
    lower.includes("multi-sub") ||
    lower.includes("multisub") ||
    lower.includes("multi sub") ||
    lower.includes("multi language") ||
    lower.includes("multilingual");

  const isEngSub =
    lower.includes("engsub") ||
    lower.includes("eng sub") ||
    lower.includes("english") ||
    isMultiSub ||
    (!lower.includes("raw") && !lower.includes(".ita.") && !lower.includes(".spa.") && !lower.includes(".ger."));

  let clean = rawTitle
    .replace(/^\[.*?\]\s*/g, "")
    .replace(/\s*\[[0-9a-fA-F]{8}\]/g, "")
    .replace(/\s*\[.*?\]/g, " ")
    .replace(/\(.*?\)/g, " ")
    .replace(/[._]/g, " ")
    .replace(/\b(19\d\d|20\d\d)\b.*$/i, "")
    .replace(
      /\b(2160p|1080p|720p|480p|4k|uhd|fhd|hd|web-dl|webrip|bluray|bdrip|remux|hdtv|dvdrip|telesync|x264|x265|hevc|av1|aac\d*|ac3|dts|flac|dual audio|engsub|vietsub|yts|nyaa|eztv|flux|sparks)\b.*$/i,
      "",
    )
    .replace(/\s*-\s*\d{1,4}\s*$/g, "")
    .replace(/\s{2,}/g, " ")
    .trim();

  if (!clean || clean.length < 2) {
    clean = rawTitle.replace(/[._]/g, " ").trim();
  }

  const richBadges: FilmBadge[] = [];
  if (qualityBadge) richBadges.push({ label: qualityBadge, category: "quality" });
  if (sourceType) richBadges.push({ label: sourceType, category: "source" });
  if (hdr) richBadges.push({ label: hdr, category: "hdr" });
  if (audio) richBadges.push({ label: audio, category: "audio" });
  if (isVietSub) richBadges.push({ label: "VietSub", category: "sub" });
  if (isThuyetMinh) richBadges.push({ label: "Thuyết Minh", category: "sub" });
  if (isMultiSub) richBadges.push({ label: "Multi-Sub", category: "sub" });
  else if (isEngSub) richBadges.push({ label: "EngSub", category: "sub" });
  if (codec) richBadges.push({ label: codec, category: "codec" });
  if (episodeInfo) richBadges.push({ label: episodeInfo, category: "source" });
  if (releaseGroup) richBadges.push({ label: releaseGroup, category: "group" });

  const tags = richBadges.map((b) => b.label);

  return {
    cleanTitle: clean,
    originalTitle: rawTitle,
    year,
    sourceType,
    quality,
    qualityBadge,
    codec,
    hdr,
    audio,
    episodeInfo,
    releaseGroup,
    isVietSub,
    isThuyetMinh,
    isEngSub,
    isMultiSub,
    tags,
    richBadges,
  };
}

const METADATA_CLIENT_CACHE = new Map<string, MediaMetadata | null>();

function useMediaMetadata(title: string, category?: string) {
  const info = useMemo(() => parseFilmReleaseInfo(title), [title]);
  const searchKey = `${category || "all"}:${info.cleanTitle.toLowerCase()}`;
  const [metadata, setMetadata] = useState<MediaMetadata | null>(() => METADATA_CLIENT_CACHE.get(searchKey) || null);

  useEffect(() => {
    let active = true;
    if (METADATA_CLIENT_CACHE.has(searchKey)) {
      setMetadata(METADATA_CLIENT_CACHE.get(searchKey) || null);
      return;
    }

    try {
      const stored = localStorage.getItem(`any-watch:meta:${searchKey}`);
      if (stored) {
        const parsed = JSON.parse(stored);
        METADATA_CLIENT_CACHE.set(searchKey, parsed);
        setMetadata(parsed);
        return;
      }
    } catch {}

    void api
      .getTorrentMetadata(info.cleanTitle, category)
      .then((res) => {
        if (!active) return;
        METADATA_CLIENT_CACHE.set(searchKey, res);
        setMetadata(res);
        if (res) {
          try {
            localStorage.setItem(`any-watch:meta:${searchKey}`, JSON.stringify(res));
          } catch {}
        }
      })
      .catch(() => {
        if (active) METADATA_CLIENT_CACHE.set(searchKey, null);
      });

    return () => {
      active = false;
    };
  }, [info.cleanTitle, category, searchKey]);

  return { info, metadata };
}

function extractMagnetHash(magnetUrl?: string): string | null {
  if (!magnetUrl || !magnetUrl.startsWith("magnet:")) return null;
  const match = magnetUrl.match(/urn:btih:([a-zA-Z0-9]+)/i);
  return match ? match[1].toLowerCase() : null;
}

function findMatchingTask(result: TorrentSearchResult, tasks: TorrentTask[]): TorrentTask | undefined {
  const resultHash = extractMagnetHash(result.magnet_url);
  return tasks.find((task) => {
    if (resultHash) {
      const taskHash = extractMagnetHash(task.magnet_url);
      if (taskHash && taskHash.toLowerCase() === resultHash.toLowerCase()) return true;
    }
    return task.title.toLowerCase().trim() === result.title.toLowerCase().trim();
  });
}

function CuratedFilmCard({
  title,
  category,
  onClick,
}: {
  title: string;
  category: string;
  onClick: () => void;
}) {
  const { metadata } = useMediaMetadata(title, category);
  const poster = metadata?.coverUrl || LOGO_SRC;

  return (
    <article className="curated-film-card" onClick={onClick} role="button" tabIndex={0}>
      <div className="curated-film-thumb">
        <img
          src={poster}
          alt={title}
          loading="lazy"
          decoding="async"
          onError={(e) => {
            (e.target as HTMLImageElement).src = LOGO_SRC;
          }}
        />
        <div className="curated-film-hover-overlay">
          <Search size={22} />
          <span>Find Releases</span>
        </div>
        {metadata?.rating && (
          <span className="curated-rating-badge">★ {metadata.rating.toFixed(1)}</span>
        )}
      </div>
      <div className="curated-film-copy">
        <strong title={title}>{title}</strong>
        <div className="curated-film-meta">
          {metadata?.year ? <span>{metadata.year}</span> : <span>Popular</span>}
          {metadata?.genres?.[0] && <span> • {metadata.genres[0]}</span>}
        </div>
      </div>
    </article>
  );
}

function CuratedFilmShelf({
  title,
  subtitle,
  icon,
  films,
  category,
  onSelectFilm,
}: {
  title: string;
  subtitle?: string;
  icon: ReactNode;
  films: string[];
  category: "all" | "anime" | "movie" | "tv" | "doc";
  onSelectFilm: (film: string, cat: "all" | "anime" | "movie" | "tv" | "doc") => void;
}) {
  const trackRef = useRef<HTMLDivElement | null>(null);

  const scroll = (direction: "left" | "right") => {
    if (!trackRef.current) return;
    trackRef.current.scrollBy({ left: direction === "left" ? -460 : 460, behavior: "smooth" });
  };

  return (
    <section className="curated-shelf-section">
      <div className="curated-shelf-header">
        <div className="curated-shelf-title-group">
          <div className="curated-shelf-icon">{icon}</div>
          <div className="curated-shelf-titles">
            <h3>{title}</h3>
            {subtitle && <p>{subtitle}</p>}
          </div>
        </div>
        <div className="curated-shelf-controls">
          <button
            type="button"
            className="curated-shelf-arrow"
            onClick={() => scroll("left")}
            aria-label={`Scroll ${title} left`}
          >
            <ChevronLeft size={16} />
          </button>
          <button
            type="button"
            className="curated-shelf-arrow"
            onClick={() => scroll("right")}
            aria-label={`Scroll ${title} right`}
          >
            <ChevronRight size={16} />
          </button>
        </div>
      </div>
      <div className="curated-shelf-track" ref={trackRef}>
        {films.map((film) => (
          <CuratedFilmCard
            key={film}
            title={film}
            category={category}
            onClick={() => onSelectFilm(film, category)}
          />
        ))}
      </div>
    </section>
  );
}

function DownloadsPage({
  onBack,
  session,
  onShowSignIn,
  onPlayFilm,
}: {
  onBack?: () => void;
  session?: SessionUser | null;
  onShowSignIn?: () => void;
  onPlayFilm?: (task: TorrentTask, metadata?: MediaMetadata | null) => void;
}) {
  const shouldReduceMotion = useReducedMotion();
  const [tab, setTab] = useState<"browse" | "history">("browse");
  const [query, setQuery] = useState("");
  const [category, setCategory] = useState<"all" | "anime" | "movie" | "tv" | "doc">("all");
  const [source, setSource] = useState("");
  const [qualityFilter, setQualityFilter] = useState<"all" | "4k" | "1080p" | "720p">("all");
  const [subPref, setSubPref] = useState<"all" | "vi" | "en">("all");
  const [results, setResults] = useState<TorrentSearchResult[]>([]);
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [tasks, setTasks] = useState<TorrentTask[]>([]);
  const [taskFilter, setTaskFilter] = useState<"all" | "ready" | "active" | "pending" | "rejected">("all");
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [actionLoadingId, setActionLoadingId] = useState<string | null>(null);
  const [deletingTaskIds, setDeletingTaskIds] = useState<Set<string>>(new Set());
  const [approvingTaskIds, setApprovingTaskIds] = useState<Set<string>>(new Set());
  const [rejectingTask, setRejectingTask] = useState<TorrentTask | null>(null);
  const [rejectReason, setRejectReason] = useState("");
  const [rejectSubmitting, setRejectSubmitting] = useState(false);

  // Live debounced search as user types
  useEffect(() => {
    const clean = query.trim();
    if (clean.length < 2) {
      setResults([]);
      setHasMore(false);
      setPage(1);
      return;
    }
    const timer = setTimeout(() => {
      void performSearch(clean, category, source, 1, false);
    }, 380);
    return () => clearTimeout(timer);
  }, [query, category, source]);

  const activeTasksCount = useMemo(() => {
    return tasks.filter(
      (t) => t.status.type === "queued" || t.status.type === "downloading" || t.status.type === "remuxing"
    ).length;
  }, [tasks]);

  const pendingRequestsCount = useMemo(() => {
    return tasks.filter((t) => t.status.type === "pending_approval").length;
  }, [tasks]);

  const rejectedRequestsCount = useMemo(() => {
    return tasks.filter((t) => t.status.type === "rejected").length;
  }, [tasks]);

  const readyTasks = useMemo(() => {
    return tasks.filter((t) => t.status.type === "ready");
  }, [tasks]);

  const totalStorageBytes = useMemo(() => {
    return readyTasks.reduce((acc, t) => acc + (t.status.type === "ready" ? t.status.data.file_size : 0), 0);
  }, [readyTasks]);

  const HOMELAB_QUOTA_BYTES = 100 * 1024 * 1024 * 1024; // 100 GB
  const remainingStorageBytes = useMemo(() => {
    return Math.max(0, HOMELAB_QUOTA_BYTES - totalStorageBytes);
  }, [totalStorageBytes]);

  const displayedResults = useMemo(() => {
    let filtered = results;
    if (subPref === "vi") {
      filtered = filtered.filter((r) => r.has_vietsub);
    } else if (subPref === "en") {
      filtered = filtered.filter((r) => r.has_engsub);
    }
    if (qualityFilter === "4k") {
      filtered = filtered.filter((r) => {
        const q = (r.quality || "").toLowerCase();
        const t = r.title.toLowerCase();
        return q.includes("4k") || q.includes("2160") || t.includes("2160p") || t.includes("4k") || t.includes("uhd");
      });
    } else if (qualityFilter === "1080p") {
      filtered = filtered.filter((r) => {
        const q = (r.quality || "").toLowerCase();
        const t = r.title.toLowerCase();
        return q.includes("1080") || t.includes("1080p") || t.includes("fhd");
      });
    } else if (qualityFilter === "720p") {
      filtered = filtered.filter((r) => {
        const q = (r.quality || "").toLowerCase();
        const t = r.title.toLowerCase();
        return q.includes("720") || t.includes("720p") || t.includes("hd");
      });
    }
    return filtered;
  }, [results, subPref, qualityFilter]);

  const displayedTasks = useMemo(() => {
    if (taskFilter === "ready") return readyTasks;
    if (taskFilter === "active")
      return tasks.filter(
        (t) => t.status.type === "queued" || t.status.type === "downloading" || t.status.type === "remuxing",
      );
    if (taskFilter === "pending") return tasks.filter((t) => t.status.type === "pending_approval");
    if (taskFilter === "rejected") return tasks.filter((t) => t.status.type === "rejected");
    return tasks;
  }, [tasks, taskFilter, readyTasks]);

  const loadTasks = async () => {
    try {
      const data = await api.listTorrentTasks();
      setTasks(data);
    } catch (err) {
      console.error("Failed to load torrent tasks:", err);
    }
  };

  useEffect(() => {
    void loadTasks();
  }, []);

  useEffect(() => {
    const hasActive = tasks.some(
      (t) => t.status.type === "queued" || t.status.type === "downloading" || t.status.type === "remuxing"
    );
    if (!hasActive && tab !== "history") return;

    const interval = setInterval(() => {
      void loadTasks();
    }, 2500);

    return () => clearInterval(interval);
  }, [tasks, tab]);

  const performSearch = async (
    targetQuery: string,
    targetCat: "all" | "anime" | "movie" | "tv" | "doc",
    targetSource: string,
    targetPage = 1,
    append = false,
  ) => {
    const cleanQuery = targetQuery.trim();
    if (cleanQuery.length < 2) return;

    if (append) setLoadingMore(true);
    else setLoading(true);
    setError(null);
    try {
      const data = await api.searchTorrents(
        cleanQuery,
        targetCat === "all" ? undefined : targetCat,
        targetSource || undefined,
        targetPage,
      );
      setPage(targetPage);
      setHasMore(data.length > 0);
      setResults((current) => {
        if (!append) return data;
        const seen = new Set(current.map((item) => item.id));
        return [...current, ...data.filter((item) => !seen.has(item.id))];
      });
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to search torrent providers.");
      if (!append) setResults([]);
    } finally {
      setLoading(false);
      setLoadingMore(false);
    }
  };

  const handleSearch = async (e?: FormEvent) => {
    if (e) e.preventDefault();
    await performSearch(query, category, source);
  };

  const handleCategoryChange = (newCat: "all" | "anime" | "movie" | "tv" | "doc") => {
    setCategory(newCat);
    if (query.trim().length >= 2) {
      void performSearch(query, newCat, source);
    }
  };

  const handleSourceChange = (newSource: string) => {
    setSource(newSource);
    if (query.trim().length >= 2) {
      void performSearch(query, category, newSource);
    }
  };

  const handleCreateTask = async (torrent: TorrentSearchResult) => {
    if (!session) {
      onShowSignIn?.();
      return;
    }
    if (torrent.size_bytes && torrent.size_bytes > remainingStorageBytes) {
      setError(
        `This release (${torrent.formatted_size}) exceeds the remaining storage quota (${formatTorrentBytes(remainingStorageBytes)} under 100 GB quota).`,
      );
      return;
    }

    setActionLoadingId(torrent.id);
    setError(null);
    try {
      const newTask = await api.createTorrentTask(
        torrent.title,
        torrent.magnet_url,
        torrent.torrent_url,
        torrent.size_bytes || undefined,
        subPref,
      );
      setTasks((current) => [newTask, ...current.filter((t) => t.id !== newTask.id)]);
      setTab("history");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to create media task.");
    } finally {
      setActionLoadingId(null);
    }
  };

  const handleApproveTask = async (id: string) => {
    setApprovingTaskIds((current) => new Set([...current, id]));
    try {
      const updated = await api.approveTorrentTask(id);
      setTasks((current) => current.map((t) => (t.id === id ? updated : t)));
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to approve media task.");
    } finally {
      setApprovingTaskIds((current) => {
        const next = new Set(current);
        next.delete(id);
        return next;
      });
    }
  };

  const handleConfirmReject = async () => {
    if (!rejectingTask) return;
    setRejectSubmitting(true);
    try {
      const reason = rejectReason.trim() || "Rejected by admin: Storage quota or release suitability";
      const updated = await api.rejectTorrentTask(rejectingTask.id, reason);
      setTasks((current) => current.map((t) => (t.id === rejectingTask.id ? updated : t)));
      setRejectingTask(null);
      setRejectReason("");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to reject media task.");
    } finally {
      setRejectSubmitting(false);
    }
  };

  const handleRejectTask = async (id: string) => {
    const task = tasks.find((t) => t.id === id);
    if (task) {
      setRejectingTask(task);
    } else {
      try {
        const updated = await api.rejectTorrentTask(id);
        setTasks((current) => current.map((t) => (t.id === id ? updated : t)));
      } catch (err: unknown) {
        setError(err instanceof Error ? err.message : "Failed to reject task.");
      }
    }
  };

  const handleDeleteTask = async (id: string) => {
    setDeletingTaskIds((current) => new Set([...current, id]));
    try {
      await api.deleteTorrentTask(id);
      setTasks((current) => current.filter((t) => t.id !== id));
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to delete task.");
    } finally {
      setDeletingTaskIds((current) => {
        const next = new Set(current);
        next.delete(id);
        return next;
      });
    }
  };

  const handleCopyMagnet = (id: string, magnet: string) => {
    if (!magnet) return;
    void navigator.clipboard?.writeText(magnet);
    setCopiedId(id);
    setTimeout(() => setCopiedId(null), 2000);
  };

  const quickSearch = (q: string, cat: "all" | "anime" | "movie" | "tv" | "doc" = "all") => {
    setQuery(q);
    setCategory(cat);
    setTab("browse");
    void performSearch(q, cat, source);
  };

  const loadMoreResults = () => {
    if (loadingMore || !hasMore) return;
    void performSearch(query, category, source, page + 1, true);
  };

  const tabsList: Array<"browse" | "history"> = ["browse", "history"];
  const selectDownloadTabFromKeyboard = (
    event: React.KeyboardEvent,
    currentTab: "browse" | "history",
  ) => {
    if (!["ArrowLeft", "ArrowRight"].includes(event.key)) return;
    event.preventDefault();
    const currentIndex = tabsList.indexOf(currentTab);
    const nextIndex =
      event.key === "ArrowRight"
        ? (currentIndex + 1) % tabsList.length
        : (currentIndex - 1 + tabsList.length) % tabsList.length;
    const nextTab = tabsList[nextIndex];
    setTab(nextTab);
    window.requestAnimationFrame(() => {
      document.getElementById(`film-${nextTab}-tab`)?.focus();
    });
  };

  return (
    <motion.section
      className="page-stage stage-downloads"
      initial="hidden"
      animate="show"
      exit="hidden"
      variants={shouldReduceMotion ? { hidden: { opacity: 0 }, show: { opacity: 1 } } : fadeUpVariant}
    >
      {/* Compact Apple-style Header */}
      <div className="film-request-compact-header">
        <div className="film-request-header-main">
          <div className="film-request-title-area">
            <div className="film-request-title-row">
              <Film size={20} className="film-header-icon" />
              <h1>Film Requests</h1>
            </div>
          </div>
        </div>

        <div className="film-request-tab-switch download-tabs" role="tablist">
          <button
            role="tab"
            id="torrent-search-tab"
            aria-controls="torrent-search-panel"
            aria-selected={tab === "browse"}
            className={`film-tab-btn download-tab-button ${tab === "browse" ? "active" : ""}`}
            onKeyDown={(event) => selectDownloadTabFromKeyboard(event, "browse")}
            onClick={() => setTab("browse")}
          >
            <Compass size={15} />
            <span>Search & Request</span>
          </button>
          <button
            role="tab"
            id="torrent-history-tab"
            aria-controls="torrent-history-panel"
            aria-selected={tab === "history"}
            className={`film-tab-btn download-tab-button ${tab === "history" ? "active" : ""}`}
            onKeyDown={(event) => selectDownloadTabFromKeyboard(event, "history")}
            onClick={() => setTab("history")}
          >
            <Clock size={15} />
            <span>Request History</span>
            {activeTasksCount + pendingRequestsCount > 0 && (
              <span className="download-tab-badge">{activeTasksCount + pendingRequestsCount}</span>
            )}
          </button>
        </div>
      </div>

      {error && (
        <div className="stage-error-banner" style={{ margin: "0.5rem 0 1rem" }}>
          <AlertTriangle size={18} />
          <span>{error}</span>
        </div>
      )}

      {tab === "browse" && (
        <div className="film-browse-container">
          {/* Prominent Search Bar & Filter Toolbar */}
          <div className="film-search-shell">
            <form
              id="torrent-search-panel"
              className="torrent-search-panel"
              role="tabpanel"
              aria-labelledby="torrent-search-tab"
              onSubmit={handleSearch}
            >
              <div className="torrent-search-bar">
                <div className="torrent-search-input-wrap">
                  <Search size={18} />
                  <input
                    type="text"
                    placeholder="Search cinema movies, anime, TV series (live search as you type)..."
                    aria-label="Search torrent indexers"
                    value={query}
                    onChange={(e) => setQuery(e.target.value)}
                    autoFocus
                  />
                  {query.length > 0 && (
                    <button
                      type="button"
                      className="torrent-search-clear"
                      onClick={() => {
                        setQuery("");
                        setResults([]);
                      }}
                      aria-label="Clear query"
                    >
                      <X size={14} />
                    </button>
                  )}
                </div>
                <button
                  type="submit"
                  className="torrent-search-submit"
                  disabled={loading || query.trim().length < 2}
                >
                  {loading ? <Loader2 size={16} className="spin" /> : <Search size={16} />}
                  <span>Search</span>
                </button>
              </div>

              {/* Site Provider Chips (Like provider filters) */}
              <div className="torrent-source-chips-bar" role="tablist" aria-label="Torrent indexer source">
                {[
                  { id: "", label: "All Indexers" },
                  { id: "YTS", label: "YTS.mx (4K Movies)" },
                  { id: "ThePirateBay", label: "The Pirate Bay" },
                  { id: "Bitsearch", label: "Bitsearch (DHT & VietSub)" },
                  { id: "EZTV", label: "EZTV (Series)" },
                  { id: "Nyaa", label: "Nyaa (Anime)" },
                  { id: "AnimeTosho", label: "AnimeTosho" },
                ].map((s) => (
                  <button
                    key={s.id}
                    type="button"
                    role="tab"
                    aria-selected={source === s.id}
                    className={`torrent-source-chip ${source === s.id ? "active" : ""}`}
                    onClick={() => handleSourceChange(s.id)}
                  >
                    {s.label}
                  </button>
                ))}
              </div>

              <div className="torrent-filters-compact-bar">
                <div className="torrent-segmented-categories">
                  {(["all", "movie", "tv", "anime", "doc"] as const).map((cat) => (
                    <button
                      key={cat}
                      type="button"
                      className={`torrent-cat-pill ${category === cat ? "active" : ""}`}
                      onClick={() => handleCategoryChange(cat)}
                    >
                      {cat === "all"
                        ? "All"
                        : cat === "movie"
                        ? "Movies"
                        : cat === "tv"
                        ? "Series"
                        : cat === "anime"
                        ? "Anime"
                        : "Docs"}
                    </button>
                  ))}
                </div>

                <div className="torrent-quick-selects">
                  <select
                    className="torrent-mini-select"
                    aria-label="Quality filter"
                    value={qualityFilter}
                    onChange={(e) => setQualityFilter(e.target.value as any)}
                  >
                    <option value="all">All Qualities</option>
                    <option value="4k">4K UHD</option>
                    <option value="1080p">1080p FHD</option>
                    <option value="720p">720p HD</option>
                  </select>

                  <select
                    className="torrent-mini-select"
                    aria-label="Subtitle filter"
                    value={subPref}
                    onChange={(e) => setSubPref(e.target.value as any)}
                  >
                    <option value="all">All Subtitles</option>
                    <option value="vi">VietSub Only</option>
                    <option value="en">EngSub Only</option>
                  </select>

                  <select
                    className="torrent-mini-select torrent-source-select"
                    aria-label="Torrent source"
                    value={source}
                    onChange={(e) => handleSourceChange(e.target.value)}
                  >
                    <option value="">All Indexers</option>
                    <option value="YTS">YTS.mx (Movies)</option>
                    <option value="ThePirateBay">The Pirate Bay</option>
                    <option value="Bitsearch">Bitsearch (DHT & VietSub)</option>
                    <option value="EZTV">EZTV (Series)</option>
                    <option value="Nyaa">Nyaa.si (Anime)</option>
                    <option value="AnimeTosho">AnimeTosho</option>
                  </select>
                </div>
              </div>
            </form>
          </div>

          {/* If searching: Show Live Results Grid */}
          {query.trim().length >= 2 ? (
            <div className="film-search-results-section">
              <div className="film-results-header">
                <h3>
                  {loading && !displayedResults.length
                    ? `Searching indexers for "${query}"...`
                    : `Releases for "${query}" (${displayedResults.length} found)`}
                </h3>
                {loading && <Loader2 size={16} className="spin" />}
              </div>

              {loading && !displayedResults.length ? (
                <div className="torrent-results-grid">
                  {Array.from({ length: 6 }).map((_, i) => (
                    <div key={i} className="torrent-result-card skeleton" style={{ minHeight: "6.5rem" }} />
                  ))}
                </div>
              ) : displayedResults.length > 0 ? (
                <div className="torrent-results-grid">
                  {displayedResults.map((item) => {
                    const isCreating = actionLoadingId === item.id;
                    const isCopied = copiedId === item.id;
                    const exceedsQuota = Boolean(item.size_bytes && item.size_bytes > remainingStorageBytes);
                    const matchingTask = findMatchingTask(item, tasks);
                    return (
                      <TorrentResultCard
                        key={item.id}
                        item={item}
                        matchingTask={matchingTask}
                        isCreating={isCreating}
                        isCopied={isCopied}
                        exceedsQuota={exceedsQuota}
                        remainingStorageBytes={remainingStorageBytes}
                        onCopy={() => handleCopyMagnet(item.id, item.magnet_url)}
                        onCreateTask={() => void handleCreateTask(item)}
                        onPlayFilm={onPlayFilm}
                      />
                    );
                  })}
                  {hasMore && (
                    <button type="button" className="torrent-load-more" onClick={loadMoreResults} disabled={loadingMore}>
                      {loadingMore ? <Loader2 size={16} className="spin" /> : <Plus size={16} />}
                      {loadingMore ? "Loading more..." : `Load page ${page + 1}`}
                    </button>
                  )}
                </div>
              ) : (
                <div className="downloads-empty" style={{ margin: "3rem auto", textAlign: "center" }}>
                  <h3>No releases found</h3>
                  <p>Try searching another title or adjusting the quality/subtitle filters above.</p>
                  <button
                    type="button"
                    className="storage-film-btn primary"
                    style={{ margin: "1rem auto 0" }}
                    onClick={() => {
                      setQualityFilter("all");
                      setSubPref("all");
                      setSource("");
                    }}
                  >
                    Reset Filters
                  </button>
                </div>
              )}
            </div>
          ) : (
            /* Dashboard View (Like YouTube and Anime watching) */
            <div className="film-dashboard-view">
              {/* 1. Storage Shelf */}
              {readyTasks.length > 0 && (
                <section className="curated-shelf-section">
                  <div className="curated-shelf-header">
                    <div className="curated-shelf-title-group">
                      <div className="curated-shelf-icon"><HardDrive size={18} /></div>
                      <div className="curated-shelf-titles">
                        <h3>Available in Shared Storage</h3>
                        <p>Fast-start MP4 streams ready to play directly in-app</p>
                      </div>
                    </div>
                    <div className="curated-shelf-controls">
                      <span className="storage-shelf-badge">{readyTasks.length} {readyTasks.length === 1 ? "film" : "films"}</span>
                    </div>
                  </div>
                  <div className="storage-dashboard-grid">
                    {readyTasks.map((task) => (
                      <StorageFilmCard
                        key={task.id}
                        task={task}
                        userRole={session?.role}
                        isDeleting={deletingTaskIds.has(task.id)}
                        onDelete={(id) => void handleDeleteTask(id)}
                        onPlayFilm={onPlayFilm}
                      />
                    ))}
                  </div>
                </section>
              )}

              {/* 2. Active Downloads & Remuxing Shelf */}
              {activeTasksCount > 0 && (
                <section className="curated-shelf-section">
                  <div className="curated-shelf-header">
                    <div className="curated-shelf-title-group">
                      <div className="curated-shelf-icon" style={{ background: "rgba(59, 130, 246, 0.2)", color: "#60a5fa" }}>
                        <Loader2 size={18} className="spin" />
                      </div>
                      <div className="curated-shelf-titles">
                        <h3>Active Downloads & Preparing</h3>
                        <p>High-speed releases currently being downloaded and remuxed</p>
                      </div>
                    </div>
                  </div>
                  <div className="torrent-tasks-list">
                    {tasks
                      .filter((t) => t.status.type === "queued" || t.status.type === "downloading" || t.status.type === "remuxing")
                      .map((task) => (
                        <TorrentTaskItem
                          key={task.id}
                          task={task}
                          userRole={session?.role}
                          isDeleting={deletingTaskIds.has(task.id)}
                          isApproving={approvingTaskIds.has(task.id)}
                          onApprove={(id) => void handleApproveTask(id)}
                          onReject={(id) => void handleRejectTask(id)}
                          onRequestReject={(t) => setRejectingTask(t)}
                          onDelete={(id) => void handleDeleteTask(id)}
                          onPlayFilm={onPlayFilm}
                        />
                      ))}
                  </div>
                </section>
              )}

              {/* 3. Trending 4K Cinema & Blockbusters Shelf */}
              <CuratedFilmShelf
                title="Trending 4K Blockbusters"
                subtitle="Top-rated Hollywood & cinema releases"
                icon={<Flame size={18} />}
                category="movie"
                films={[
                  "Dune: Part Two",
                  "Oppenheimer",
                  "Interstellar",
                  "Inception",
                  "Spider-Man: Across the Spider-Verse",
                  "Avatar: The Way of Water",
                  "Blade Runner 2049",
                  "Top Gun: Maverick",
                  "The Dark Knight",
                  "Everything Everywhere All at Once"
                ]}
                onSelectFilm={quickSearch}
              />

              {/* 4. Top Anime Movies & Series Shelf */}
              <CuratedFilmShelf
                title="Popular Anime Movies & Series"
                subtitle="Acclaimed anime titles with master quality and subs"
                icon={<Sparkles size={18} />}
                category="anime"
                films={[
                  "Frieren: Beyond Journey's End",
                  "Demon Slayer: Kimetsu no Yaiba",
                  "Attack on Titan",
                  "Jujutsu Kaisen",
                  "Your Name",
                  "Spirited Away",
                  "Chainsaw Man",
                  "Solo Leveling",
                  "Suzume",
                  "A Silent Voice"
                ]}
                onSelectFilm={quickSearch}
              />

              {/* 5. Acclaimed TV Series Shelf */}
              <CuratedFilmShelf
                title="Acclaimed TV Series"
                subtitle="Top-tier television series available across indexers"
                icon={<Tv size={18} />}
                category="tv"
                films={[
                  "Arcane",
                  "Breaking Bad",
                  "Shogun",
                  "Game of Thrones",
                  "The Last of Us",
                  "Stranger Things",
                  "Succession",
                  "Better Call Saul"
                ]}
                onSelectFilm={quickSearch}
              />
            </div>
          )}
        </div>
      )}

      {/* Request History Tab */}
      {tab === "history" && (
        <div
          id="torrent-history-panel"
          className="torrent-tasks-workspace"
          role="tabpanel"
          aria-labelledby="film-history-tab"
        >
          <div className="storage-overview-banner" style={{ marginBottom: "1rem" }}>
            <div className="storage-stat-pill">
              <HardDrive size={16} />
              <span>
                <strong>{readyTasks.length}</strong> {readyTasks.length === 1 ? "film" : "films"} ready
              </span>
            </div>
            <div className="storage-stat-pill">
              <span>
                Storage: <strong>{formatTorrentBytes(totalStorageBytes)}</strong> / 100 GB (
                <strong>{formatTorrentBytes(remainingStorageBytes)}</strong> free)
              </span>
            </div>
          </div>

          <div className="storage-quota-bar-wrapper" style={{ marginBottom: "1.25rem" }}>
            <div className="storage-quota-bar">
              <div
                className="storage-quota-bar-fill"
                style={{ width: `${Math.min(100, Math.max(0, (totalStorageBytes / HOMELAB_QUOTA_BYTES) * 100))}%` }}
              />
            </div>
          </div>

          <div className="workspace-filter-pills" style={{ display: "flex", gap: "0.5rem", marginBottom: "1.25rem", flexWrap: "wrap" }}>
            <button
              type="button"
              className={`torrent-filter-chip ${taskFilter === "all" ? "active" : ""}`}
              onClick={() => setTaskFilter("all")}
            >
              All Requests ({tasks.length})
            </button>
            <button
              type="button"
              className={`torrent-filter-chip ${taskFilter === "ready" ? "active" : ""}`}
              onClick={() => setTaskFilter("ready")}
            >
              Ready to Watch ({readyTasks.length})
            </button>
            <button
              type="button"
              className={`torrent-filter-chip ${taskFilter === "active" ? "active" : ""}`}
              onClick={() => setTaskFilter("active")}
            >
              Downloading & Active ({activeTasksCount})
            </button>
            <button
              type="button"
              className={`torrent-filter-chip ${taskFilter === "pending" ? "active" : ""}`}
              onClick={() => setTaskFilter("pending")}
            >
              Pending Approval ({pendingRequestsCount})
            </button>
            <button
              type="button"
              className={`torrent-filter-chip ${taskFilter === "rejected" ? "active" : ""}`}
              onClick={() => setTaskFilter("rejected")}
            >
              Rejected ({rejectedRequestsCount})
            </button>
          </div>

          {displayedTasks.length > 0 ? (
            <div className="torrent-tasks-list">
              {displayedTasks.map((task) => (
                <TorrentTaskItem
                  key={task.id}
                  task={task}
                  userRole={session?.role}
                  isDeleting={deletingTaskIds.has(task.id)}
                  isApproving={approvingTaskIds.has(task.id)}
                  onApprove={(id) => void handleApproveTask(id)}
                  onReject={(id) => void handleRejectTask(id)}
                  onRequestReject={(t) => setRejectingTask(t)}
                  onDelete={(id) => void handleDeleteTask(id)}
                  onPlayFilm={onPlayFilm}
                />
              ))}
            </div>
          ) : (
            <div className="downloads-empty" style={{ margin: "3rem auto", textAlign: "center" }}>
              <div>
                <Clock size={26} />
              </div>
              <h3>No Requests in This View</h3>
              <p>Your request history is currently clean. Switch to Search & Library to find and queue films or anime.</p>
            </div>
          )}
        </div>
      )}

      {/* Admin Rejection Modal */}
      {rejectingTask && (
        <div className="login-modal-overlay" onClick={() => setRejectingTask(null)}>
          <div
            className="login-modal-dialog"
            onClick={(e) => e.stopPropagation()}
            style={{ maxWidth: "28rem", padding: "1.5rem" }}
          >
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "1rem" }}>
              <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
                <AlertTriangle size={18} color="#f87171" />
                <h3 style={{ margin: 0, fontSize: "var(--text-lg)" }}>Reject Film Request</h3>
              </div>
              <button
                type="button"
                className="torrent-search-clear"
                onClick={() => setRejectingTask(null)}
                aria-label="Close dialog"
              >
                <X size={16} />
              </button>
            </div>

            <p style={{ margin: "0 0 1rem", fontSize: "var(--text-sm)", color: "var(--color-muted)" }}>
              Specify the rejection reason for "<strong>{rejectingTask.title}</strong>". The requester will be able to see
              this explanation in their Request History.
            </p>

            <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem", marginBottom: "1rem" }}>
              <button
                type="button"
                className="storage-film-btn"
                onClick={() => setRejectReason("Insufficient storage quota (Homelab storage full)")}
                style={{ textAlign: "left", justifyContent: "flex-start" }}
              >
                1. Insufficient storage quota (100GB quota reached)
              </button>
              <button
                type="button"
                className="storage-film-btn"
                onClick={() => setRejectReason("Duplicate release already available in Shared Storage")}
                style={{ textAlign: "left", justifyContent: "flex-start" }}
              >
                2. Duplicate release already available in storage
              </button>
              <button
                type="button"
                className="storage-film-btn"
                onClick={() => setRejectReason("Low seeder health / slow torrent source")}
                style={{ textAlign: "left", justifyContent: "flex-start" }}
              >
                3. Low seeder health / dead source
              </button>
              <button
                type="button"
                className="storage-film-btn"
                onClick={() => setRejectReason("Low audio/video quality or unsupported release format")}
                style={{ textAlign: "left", justifyContent: "flex-start" }}
              >
                4. Low audio/video quality release
              </button>
            </div>

            <label style={{ display: "block", fontSize: "var(--text-xs)", marginBottom: "0.35rem", color: "var(--color-muted)" }}>
              Custom Reason Note:
            </label>
            <textarea
              rows={3}
              value={rejectReason}
              onChange={(e) => setRejectReason(e.target.value)}
              placeholder="e.g. Please search for a 1080p release instead of 4K 80GB..."
              style={{
                width: "100%",
                padding: "0.6rem",
                borderRadius: "var(--radius-input)",
                background: "var(--color-paper-2)",
                border: "1px solid var(--color-glass-hairline)",
                color: "var(--color-ink)",
                resize: "vertical",
                fontSize: "var(--text-sm)",
                marginBottom: "1.25rem",
              }}
            />

            <div style={{ display: "flex", gap: "0.75rem", justifyContent: "flex-end" }}>
              <button
                type="button"
                className="storage-film-btn"
                onClick={() => setRejectingTask(null)}
                disabled={rejectSubmitting}
              >
                Cancel
              </button>
              <button
                type="button"
                className="storage-film-btn"
                onClick={handleConfirmReject}
                disabled={rejectSubmitting}
                style={{ background: "rgba(239, 68, 68, 0.2)", borderColor: "#ef4444", color: "#f87171", fontWeight: 700 }}
              >
                {rejectSubmitting ? <Loader2 size={14} className="spin" /> : <X size={14} />}
                <span>{rejectSubmitting ? "Rejecting..." : "Confirm Rejection"}</span>
              </button>
            </div>
          </div>
        </div>
      )}
    </motion.section>
  );
}

function TorrentResultCard({
  item,
  matchingTask,
  isCreating,
  isCopied,
  exceedsQuota,
  remainingStorageBytes,
  onCopy,
  onCreateTask,
  onPlayFilm,
}: {
  item: TorrentSearchResult;
  matchingTask?: TorrentTask;
  isCreating: boolean;
  isCopied: boolean;
  exceedsQuota: boolean;
  remainingStorageBytes: number;
  onCopy: () => void;
  onCreateTask: () => void;
  onPlayFilm?: (task: TorrentTask, metadata?: MediaMetadata | null) => void;
}) {
  const { info, metadata } = useMediaMetadata(item.title, item.category);
  const isReady = matchingTask?.status.type === "ready";
  const isDownloading = matchingTask?.status.type === "downloading";
  const isRemuxing = matchingTask?.status.type === "remuxing";
  const isQueued = matchingTask?.status.type === "queued";
  const isPending = matchingTask?.status.type === "pending_approval";

  return (
    <div className={`torrent-result-card ${exceedsQuota ? "quota-exceeded" : ""} ${isReady ? "ready" : ""}`}>
      {metadata?.coverUrl && (
        <div
          className="torrent-result-thumb-wrap"
          style={{
            width: "4.25rem",
            height: "6rem",
            flexShrink: 0,
            borderRadius: "var(--radius-input)",
            overflow: "hidden",
          }}
        >
          <img
            src={metadata.coverUrl}
            alt={info.cleanTitle}
            style={{ width: "100%", height: "100%", objectFit: "cover" }}
            onError={(e) => {
              (e.target as HTMLImageElement).style.display = "none";
            }}
          />
        </div>
      )}
      <div className="torrent-result-info">
        <h3 className="torrent-result-title" title={item.title}>
          {info.cleanTitle} {info.year ? `(${info.year})` : ""}
        </h3>
        <p
          className="torrent-result-raw-name"
          style={{
            margin: "0.15rem 0 0.35rem",
            fontSize: "0.72rem",
            color: "var(--color-muted)",
            opacity: 0.8,
          }}
          title={item.title}
        >
          {item.title}
        </p>
        <div className="torrent-meta-badges">
          <span className={`badge-source ${item.source.toLowerCase().replace(/^the/, "")}`}>{item.source}</span>
          <span className="badge-pill badge-cat">{item.category}</span>
          {info.richBadges.map((badge, idx) => (
            <span key={idx} className={`badge-pill badge-${badge.category}`}>
              {badge.label}
            </span>
          ))}
          <span className="badge-pill badge-size">{item.formatted_size}</span>
          <span className={`badge-pill badge-health ${item.seeds >= 20 ? "high" : item.seeds >= 5 ? "medium" : "low"}`}>
            <span className="seeds">▲ {item.seeds}</span>
            <span className="peers">▼ {item.peers}</span>
          </span>
          {metadata?.rating && <span className="badge-pill badge-rating">★ {metadata.rating.toFixed(1)}</span>}
          {exceedsQuota && !isReady && <span className="badge-pill badge-danger">Exceeds Quota</span>}
        </div>
      </div>

      <div className="torrent-result-actions">
        <button
          type="button"
          className="torrent-btn-icon"
          onClick={onCopy}
          title={isCopied ? "Magnet link copied to clipboard" : "Copy Magnet link"}
          aria-label={isCopied ? "Magnet link copied to clipboard" : "Copy Magnet link"}
        >
          {isCopied ? <Check size={16} /> : <Copy size={16} />}
        </button>

        {isReady && matchingTask ? (
          <button
            type="button"
            className="storage-film-btn primary"
            onClick={() => onPlayFilm?.(matchingTask, metadata)}
            title="Play film in built-in any-watch player"
            style={{ padding: "0.55rem 1rem", fontSize: "var(--text-sm)" }}
          >
            <Play size={16} fill="currentColor" />
            <span>Play Film</span>
          </button>
        ) : isDownloading && matchingTask?.status.type === "downloading" ? (
          <span className="torrent-task-status-badge downloading" style={{ padding: "0.45rem 0.75rem" }}>
            <Loader2 size={14} className="spin" />
            <span>Downloading ({Math.round(matchingTask.status.data.progress * 100)}%)</span>
          </span>
        ) : isRemuxing ? (
          <span className="torrent-task-status-badge remuxing" style={{ padding: "0.45rem 0.75rem" }}>
            <Loader2 size={14} className="spin" />
            <span>Preparing MP4...</span>
          </span>
        ) : isQueued ? (
          <span className="torrent-task-status-badge queued" style={{ padding: "0.45rem 0.75rem" }}>
            <span>In Queue</span>
          </span>
        ) : isPending ? (
          <span className="torrent-task-status-badge pending_approval" style={{ padding: "0.45rem 0.75rem" }}>
            <Clock size={14} />
            <span>Pending Approval</span>
          </span>
        ) : (
          <button
            type="button"
            className="torrent-btn-download torrent-btn-request"
            onClick={onCreateTask}
            disabled={isCreating || exceedsQuota}
            title={
              exceedsQuota
                ? `Exceeds available storage (${formatTorrentBytes(remainingStorageBytes)})`
                : "Request this film for shared storage"
            }
          >
            {isCreating ? <Loader2 size={15} className="spin" /> : <Download size={15} />}
            <span>{exceedsQuota ? "Exceeds Quota" : "Request Film"}</span>
          </button>
        )}
      </div>
    </div>
  );
}

function StorageFilmCard({
  task,
  userRole,
  isDeleting,
  onDelete,
  onPlayFilm,
}: {
  task: TorrentTask;
  userRole?: "admin" | "user";
  isDeleting?: boolean;
  onDelete: (id: string) => void;
  onPlayFilm?: (task: TorrentTask, metadata?: MediaMetadata | null) => void;
}) {
  const { info, metadata } = useMediaMetadata(task.title);
  const status = task.status;
  if (status.type !== "ready") return null;

  const isAdmin = userRole === "admin";
  const canDelete = isAdmin;
  const posterImg = metadata?.coverUrl || LOGO_SRC;

  return (
    <article className="torrent-task-card storage-film-card ready">
      <div className="storage-film-poster-wrap">
        <img
          src={posterImg}
          alt={info.cleanTitle}
          onError={(e) => {
            (e.target as HTMLImageElement).src = LOGO_SRC;
          }}
        />
        <div className="storage-film-badges">
          <span className="torrent-task-status-badge ready">
            <Check size={12} /> Ready
          </span>
          {info.qualityBadge && <span className="storage-badge quality">{info.qualityBadge}</span>}
          {info.isVietSub && <span className="storage-badge vietsub">VietSub</span>}
          {info.isEngSub && <span className="storage-badge engsub">EngSub</span>}
          {metadata?.rating && (
            <span className="storage-badge rating" style={{ background: "rgba(0,0,0,0.78)", color: "#facc15" }}>
              ★ {metadata.rating.toFixed(1)}
            </span>
          )}
        </div>
        <div
          className="storage-film-play-overlay"
          onClick={() => onPlayFilm?.(task, metadata)}
          role="button"
          tabIndex={0}
          aria-label={`Play ${info.cleanTitle}`}
        >
          <div className="storage-film-play-btn">
            <Play size={22} fill="currentColor" />
          </div>
        </div>
      </div>

      <div className="storage-film-info">
        <h3 className="storage-film-title torrent-task-title" title={task.title}>
          {info.cleanTitle}
        </h3>
        <div className="storage-film-meta">
          <span>
            {info.year || metadata?.year ? `${info.year || metadata?.year} • ` : ""}
            {formatTorrentBytes(status.data.file_size)}
          </span>
          {info.codec && <span>{info.codec}</span>}
        </div>

        {metadata?.genres && metadata.genres.length > 0 && (
          <div
            className="storage-film-genres"
            style={{ display: "flex", gap: "0.25rem", flexWrap: "wrap", marginTop: "0.25rem" }}
          >
            {metadata.genres.slice(0, 3).map((g) => (
              <span
                key={g}
                className="genre-pill"
                style={{
                  fontSize: "0.68rem",
                  opacity: 0.8,
                  padding: "0.1rem 0.35rem",
                  borderRadius: "4px",
                  background: "var(--color-paper-3)",
                }}
              >
                {g}
              </span>
            ))}
          </div>
        )}

        <div className="storage-film-actions torrent-task-actions-row">
          <button
            type="button"
            className="storage-film-btn primary"
            onClick={() => onPlayFilm?.(task, metadata)}
            title="Play film in built-in player"
          >
            <Play size={14} fill="currentColor" />
            <span>Play Film</span>
          </button>
          {canDelete && (
            <button
              type="button"
              className="storage-film-btn torrent-btn-delete"
              style={{ flex: "0 0 auto", color: "var(--color-danger)" }}
              disabled={isDeleting}
              onClick={() => onDelete(task.id)}
              title="Delete from storage"
            >
              {isDeleting ? <Loader2 size={14} className="spin" /> : <Trash2 size={14} />}
              <span>Delete</span>
            </button>
          )}
        </div>
      </div>
    </article>
  );
}

function TorrentTaskItem({
  task,
  userRole,
  isDeleting,
  isApproving,
  onApprove,
  onReject,
  onRequestReject,
  onDelete,
  onPlayFilm,
}: {
  task: TorrentTask;
  userRole?: "admin" | "user";
  isDeleting?: boolean;
  isApproving?: boolean;
  onApprove?: (id: string) => void;
  onReject?: (id: string) => void;
  onRequestReject?: (task: TorrentTask) => void;
  onDelete: (id: string) => void;
  onPlayFilm?: (task: TorrentTask, metadata?: MediaMetadata | null) => void;
}) {
  const status = task.status;
  const isPending = status.type === "pending_approval";
  const isReady = status.type === "ready";
  const isFailed = status.type === "failed";
  const isRejected = status.type === "rejected";
  const isAdmin = userRole === "admin";
  const canDelete = isAdmin || isRejected || isFailed;
  const { info, metadata } = useMediaMetadata(task.title);

  return (
    <div
      className={`torrent-task-card ${
        isReady ? "ready" : isFailed ? "failed" : isRejected ? "rejected" : isPending ? "pending" : ""
      }`}
    >
      <div className="torrent-task-header">
        {metadata?.coverUrl && (
          <div
            className="torrent-result-thumb-wrap"
            style={{
              width: "3.5rem",
              height: "5rem",
              flexShrink: 0,
              borderRadius: "var(--radius-input)",
              overflow: "hidden",
            }}
          >
            <img
              src={metadata.coverUrl}
              alt={info.cleanTitle}
              style={{ width: "100%", height: "100%", objectFit: "cover" }}
              onError={(e) => {
                (e.target as HTMLImageElement).style.display = "none";
              }}
            />
          </div>
        )}
        <div className="torrent-task-title-group">
          <h3 className="torrent-task-title" title={task.title}>
            {info.cleanTitle} {info.year ? `(${info.year})` : ""}
          </h3>
          <span className="torrent-task-date">
            Task ID: {task.id.slice(0, 8)} • Added {new Date(task.created_at * 1000).toLocaleTimeString()}
            {isPending && ` • Requested by ${status.data.requester_name || status.data.requester_id}`}
            {isRejected && status.data.requester_name && ` • Requested by ${status.data.requester_name}`}
          </span>
          {isReady && status.type === "ready" && (
            <div className="torrent-meta-badges" style={{ marginTop: "0.35rem" }}>
              <span className="badge-pill badge-size">{formatTorrentBytes(status.data.file_size)}</span>
              {info.qualityBadge && <span className="badge-pill badge-quality">{info.qualityBadge}</span>}
              {info.isVietSub && <span className="badge-pill badge-sub vi">VietSub</span>}
              {info.isEngSub && <span className="badge-pill badge-sub en">EngSub</span>}
              {metadata?.rating && <span className="badge-pill badge-rating">★ {metadata.rating.toFixed(1)}</span>}
            </div>
          )}
        </div>

        <div className={`torrent-task-status-badge ${status.type}`}>
          {status.type === "pending_approval" && (
            <span>
              <Clock size={13} /> {isAdmin ? `Pending Approval (${status.data.requester_name})` : "Pending Admin Approval"}
            </span>
          )}
          {status.type === "queued" && <span>In Queue</span>}
          {status.type === "downloading" && (
            <span>
              <Loader2 size={13} className="spin" /> Downloading ({Math.round(status.data.progress * 100)}%)
            </span>
          )}
          {status.type === "remuxing" && (
            <span>
              <Loader2 size={13} className="spin" /> Preparing MP4...
            </span>
          )}
          {status.type === "ready" && (
            <span>
              <Check size={13} /> Ready to Watch
            </span>
          )}
          {status.type === "rejected" && (
            <span>
              <X size={13} /> Rejected
            </span>
          )}
          {status.type === "failed" && (
            <span>
              <AlertTriangle size={13} /> Failed
            </span>
          )}
        </div>
      </div>

      {isPending && (
        <div className="stage-note" style={{ margin: "0.25rem 0" }}>
          <span>
            {isAdmin
              ? `Film request submitted by viewer "${status.data.requester_name}". Approve to start downloading and preparing media immediately.`
              : "Your request has been submitted to the administrator. Once approved, it will be prepared for in-app playback."}
          </span>
        </div>
      )}

      {isRejected && (
        <div
          className="stage-error-banner rejection-banner"
          style={{
            margin: "0.4rem 0",
            background: "rgba(239, 68, 68, 0.12)",
            borderColor: "rgba(239, 68, 68, 0.35)",
            color: "#f87171",
          }}
        >
          <AlertTriangle size={16} />
          <div>
            <strong>Request Rejected by Admin:</strong>
            <p style={{ margin: "0.15rem 0 0", fontSize: "var(--text-xs)", opacity: 0.95 }}>{status.data.reason}</p>
          </div>
        </div>
      )}

      {status.type === "downloading" && (
        <div className="torrent-task-progress-shell">
          <div className="torrent-task-bar">
            <div className="torrent-task-bar-fill" style={{ width: `${Math.round(status.data.progress * 100)}%` }} />
          </div>
          <div className="torrent-task-stats-row">
            <span>
              Speed: <strong>{formatSpeed(status.data.speed_bytes_per_sec)}</strong>
            </span>
            <span>
              {formatTorrentBytes(status.data.downloaded_bytes)} / {formatTorrentBytes(status.data.total_bytes)}
            </span>
            <span>
              ETA: <strong>{formatEta(status.data.eta_seconds)}</strong>
            </span>
          </div>
        </div>
      )}

      {status.type === "remuxing" && (
        <div className="torrent-task-progress-shell">
          <div className="torrent-task-bar">
            <div className="torrent-task-bar-fill" style={{ width: `${Math.round(status.data.progress * 100)}%` }} />
          </div>
          <div className="torrent-task-stats-row">
            <span>{status.data.message}</span>
          </div>
        </div>
      )}

      {status.type === "failed" && (
        <div className="stage-error-banner" style={{ margin: "0.25rem 0" }}>
          <AlertTriangle size={16} />
          <span>{status.data.reason}</span>
        </div>
      )}

      <div className="torrent-task-actions-row">
        {isPending && isAdmin && onApprove && (
          <button
            type="button"
            className="torrent-btn-approve"
            disabled={isApproving || isDeleting}
            onClick={() => onApprove(task.id)}
          >
            {isApproving ? <Loader2 size={14} className="spin" /> : <Check size={14} />}
            <span>{isApproving ? "Approving..." : "Approve Download"}</span>
          </button>
        )}
        {isPending && isAdmin && (onRequestReject || onReject) && (
          <button
            type="button"
            className="torrent-btn-delete"
            disabled={isApproving || isDeleting}
            onClick={() => (onRequestReject ? onRequestReject(task) : onReject?.(task.id))}
            style={{
              background: "rgba(239, 68, 68, 0.15)",
              borderColor: "rgba(239, 68, 68, 0.4)",
              color: "#f87171",
            }}
          >
            <X size={14} />
            <span>Reject</span>
          </button>
        )}
        {status.type === "ready" && (
          <button
            type="button"
            className="storage-film-btn primary"
            onClick={() => onPlayFilm?.(task, metadata)}
            style={{ padding: "0.5rem 1.1rem", fontSize: "var(--text-sm)" }}
          >
            <Play size={15} fill="currentColor" />
            <span>Play in any-watch</span>
          </button>
        )}
        {canDelete && (
          <button
            type="button"
            className="torrent-btn-delete"
            disabled={isDeleting || isApproving}
            onClick={() => onDelete(task.id)}
            title="Delete task from storage"
          >
            {isDeleting ? <Loader2 size={14} className="spin" /> : <Trash2 size={14} />}
            <span>Delete</span>
          </button>
        )}
      </div>
    </div>
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
    { id: "oled", name: "OLED Theatre", description: "Deeper pitch black for dark rooms and OLED displays." },
    { id: "ember", name: "Ember Room", description: "Warm charcoal with radiant vermilion amber glow." },
    { id: "crimson", name: "Crimson Noir", description: "Wine-black surfaces with richer theatrical ruby red." },
    { id: "tokyo", name: "Tokyo Night", description: "Deep midnight indigo with vibrant magenta and cyan accents." },
    { id: "cyberpunk", name: "Cyberpunk Neon", description: "Dark cyber surfaces with electric neon yellow & teal." },
    { id: "emerald", name: "Emerald Forest", description: "Deep obsidian-jade with luminous mint accents." },
    { id: "amethyst", name: "Amethyst Violet", description: "Royal twilight with vibrant lilac & purple glow." },
    { id: "sunset", name: "Sunset Velvet", description: "Dark espresso with warm amber & coral tones." },
    { id: "nordic", name: "Nordic Frost", description: "Deep arctic navy with crystal ice blue accents." },
    { id: "system", name: "Device Contrast", description: "Follows the device system contrast preference." },
  ];
  const scales: Array<{ id: AppScale; name: string; description: string }> = [
    { id: "compact", name: "Compact", description: "More titles and controls on a 16-inch display." },
    { id: "comfortable", name: "Comfortable", description: "Balanced spacing for everyday viewing." },
    { id: "large", name: "Large", description: "Larger text and touch targets for shared screens." },
    { id: "tv", name: "TV / remote", description: "10-foot text, generous safe margins, and arrow-key focus navigation." },
  ];
  const fonts: Array<{ id: AppFont; name: string; description: string }> = [
    { id: "manrope", name: "Manrope", description: "Modern interface geometric font with Vietnamese support." },
    { id: "noto", name: "Noto Sans", description: "Highly legible Vietnamese and multilingual typography." },
    { id: "vietnam", name: "Be Vietnam Pro", description: "Carefully engineered for elegant Vietnamese diacritics." },
    { id: "jakarta", name: "Plus Jakarta Sans", description: "Clean, crisp contemporary geometric grotesque." },
    { id: "outfit", name: "Outfit", description: "Distinctive modern display and UI typography." },
    { id: "mono", name: "JetBrains / IBM Mono", description: "Precision cinematic technical monospace." },
    { id: "system", name: "System Native", description: "Native system font on macOS, iPhone, or browser." },
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
  onClose,
}: {
  error: string | null;
  onLogin: (username: string, password: string) => Promise<void>;
  onClose?: () => void;
}) {
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [submitting, setSubmitting] = useState(false);

  return (
    <main className="login-screen">
      <div className="login-ambient" />
      <section className="login-showcase" aria-label="any-watch theatre">
        <div className="login-showcase-brand">
          <img src={LOGO_SRC} alt="" />
          <span>any-watch</span>
        </div>
        <div className="login-showcase-copy">
          <p>Private streaming theatre</p>
          <h1>Pick a source.<br />Keep your place.</h1>
          <span>One synchronized watchlist with provider-specific search and episode progress across all your devices.</span>
        </div>
        <dl className="login-showcase-facts">
          <div><dt>Catalogs</dt><dd>Provider-first</dd></div>
          <div><dt>Access</dt><dd>Member accounts</dd></div>
          <div><dt>Playback</dt><dd>Any browser</dd></div>
        </dl>
      </section>
      <motion.section
        className="login-card"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.22 }}
      >
        {onClose && (
          <button
            type="button"
            className="login-modal-close"
            onClick={onClose}
            aria-label="Close sign in"
          >
            <X size={20} />
          </button>
        )}
        <div className="login-brand">
          <img src={LOGO_SRC} alt="any-watch" />
          <div><span>any-watch</span><small>Signed-in access</small></div>
        </div>
        <div className="login-copy">
          <p className="eyebrow">Private watch space</p>
          <h2>Sign in</h2>
          <p>Use the account created by your administrator.</p>
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
  onShowDownloads,
  onShowSettings,
  session,
  onShowAdmin,
  onSignOut,
  onShowSignIn,
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
  onShowDownloads: () => void;
  onShowSettings: () => void;
  session?: SessionUser | null;
  onShowAdmin?: () => void;
  onSignOut?: () => void;
  onShowSignIn?: () => void;
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
      description: plainDescription(item.description) || "Open the title, choose a provider, and see available episodes.",
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
    context: "Private theatre",
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
            <button onClick={onShowDownloads}><Download size={16} /> Downloads</button>
            <button onClick={onShowSettings}><Settings2 size={16} /> Settings</button>
            {onShowAdmin && <button onClick={onShowAdmin}><ShieldCheck size={16} /> Users</button>}
            {session ? (
              onSignOut && <button onClick={onSignOut}><LogOut size={16} /> Sign out {session.username}</button>
            ) : (
              onShowSignIn && <button className="primary nav-signin-btn" onClick={onShowSignIn}><LogIn size={16} /> Sign in</button>
            )}
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
          onOpenSearch={onOpenSearch}
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
  const activeSources = sources.filter(
    (s) => isSourceActive(s) && s.languageGroup !== "youtube",
  );
  if (!activeSources.length) return <p className="source-empty">No providers enabled.</p>;

  return (
    <div className="provider-strip" aria-label="Search providers">
      {activeSources.map((source) => (
        <button
          key={source.name}
          className={selected?.name === source.name ? "provider-chip active" : "provider-chip"}
          aria-pressed={selected?.name === source.name}
          onClick={() => onSelect(source)}
        >
          <strong>{serverLabel(source.name, sources)}</strong>
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
  onOpenSearch,
}: {
  items: WatchHistory[];
  total: number;
  onOpen: (item: WatchHistory) => void;
  onShowMore?: () => void;
  myList: Favorite[];
  onToggleFavorite: (item: WatchHistory) => void;
  onRemove: (item: WatchHistory) => void;
  onOpenSearch?: () => void;
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
          <ShelfEmptyCard
            title="Nothing to resume"
            subtitle="Start an episode and it will appear here."
            actionLabel="Find something to watch"
            onAction={onOpenSearch}
          />
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
            : (
              <ShelfEmptyCard title={emptyTitle} subtitle={emptySubtitle} />
            )}
      </div>
    </motion.section>
  );
}

function ShelfEmptyCard({
  title,
  subtitle,
  actionLabel,
  onAction,
}: {
  title: string;
  subtitle: string;
  actionLabel?: string;
  onAction?: () => void;
}) {
  return (
    <div className="shelf-empty-card">
      <img src={LOGO_SRC} alt="" />
      <div>
        <strong>{title}</strong>
        <span>{subtitle}</span>
      </div>
      {actionLabel && onAction && (
        <button
          type="button"
          className="shelf-empty-action-btn"
          style={{
            marginLeft: "auto",
            padding: "0.4rem 0.85rem",
            borderRadius: "var(--radius-pill)",
            border: "1px solid var(--color-glass-hairline)",
            background: "var(--color-paper-3)",
            color: "var(--color-ink)",
            fontSize: "var(--text-xs)",
            fontWeight: 700,
            cursor: "pointer",
            display: "inline-flex",
            alignItems: "center",
            gap: "0.35rem",
            whiteSpace: "nowrap",
          }}
          onClick={onAction}
        >
          <Search size={13} />
          <span>{actionLabel}</span>
        </button>
      )}
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
  const [catalogError, setCatalogError] = useState<string | null>(null);
  const [reloadGeneration, setReloadGeneration] = useState(0);
  const currentYear = new Date().getFullYear();
  const networkSort = sort === "personalMatch" ? "trending" : sort;
  const visibleItems = useMemo(() => {
    if (sort !== "personalMatch") return items;
    return sortCatalogByPersonalMatch(items);
  }, [items, sort]);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setCatalogError(null);
    void api.getCatalog(filters, networkSort, page)
      .then((result) => {
        if (cancelled) return;
        setItems(result.items);
        setHasNextPage(result.hasNextPage);
      })
      .catch((error) => {
        if (cancelled) return;
        setItems([]);
        setHasNextPage(false);
        setCatalogError(toAppError(error, "catalog").message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => { cancelled = true; };
  }, [filters.genre, filters.season, filters.year, filters.format, filters.status, networkSort, page, reloadGeneration]);

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
      {catalogError && (
        <div className="catalog-error" role="alert">
          <AlertTriangle size={18} />
          <span>{catalogError}</span>
          <button type="button" onClick={() => setReloadGeneration((value) => value + 1)}>Retry</button>
        </div>
      )}
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
  suggestedCatalog,
  onQueryChange,
  onSearch,
  onLanguageChange,
  onProviderSelect,
  onProviderSourceSelect,
  onProviderHealthRetry,
  providerHealthPending,
  onSelectProviderResult,
  onSelectCatalog,
  onOpenCatalog,
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
  suggestedCatalog: CatalogAnime[];
  onQueryChange: (query: string) => void;
  onSearch: (targetQuery?: string) => void;
  onLanguageChange: (language: "english" | "vietnamese") => void;
  onProviderSelect: (option: ProviderAvailability) => void;
  onProviderSourceSelect: (source: Source) => void;
  onProviderHealthRetry: (source: Source) => void;
  providerHealthPending: string | null;
  onSelectProviderResult: (anime: Anime) => void;
  onSelectCatalog: (anime: CatalogAnime) => void;
  onOpenCatalog: (anime: CatalogAnime) => void;
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
  const activeLanguageSources = sources.filter(
    (source) => source.languageGroup === languageGroup && isSourceActive(source),
  );
  const languageSources = activeLanguageSources.length > 0
    ? activeLanguageSources
    : sources.filter(
        (source) => source.languageGroup === languageGroup && source.status !== "unavailable" && Boolean(source.capabilities?.search),
      );
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

  const [providerCatalog, setProviderCatalog] = useState<Anime[]>([]);
  const [providerCatalogLoading, setProviderCatalogLoading] = useState(false);
  const [providerCatalogError, setProviderCatalogError] = useState(false);
  const fallbackSuggestions = suggestedCatalog.slice(0, 12);
  const curatedTopics = languageGroup === "vietnamese"
    ? ["One Piece", "Naruto", "Solo Leveling", "Thanh Gươm Diệt Quỷ", "Đại Chiến Titan", "Thám Tử Conan", "Phim Mới", "Tình Cảm"]
    : ["One Piece", "Attack on Titan", "Demon Slayer", "Jujutsu Kaisen", "Your Name", "Death Note", "Action", "Fantasy"];
  const providerTopics = [...new Set([
    ...providerCatalog.slice(0, 8).map((item) => item.title),
    ...curatedTopics,
  ])].slice(0, 12);

  function getCachedProviderCatalog(name: string): { items: Anime[]; isFresh: boolean } | null {
    try {
      const raw = localStorage.getItem(`any-watch:provider-catalog-v4:${name}`);
      if (!raw) return null;
      const parsed = JSON.parse(raw);
      if (parsed && Array.isArray(parsed.items) && typeof parsed.timestamp === "number" && parsed.items.length > 0) {
        const isFresh = Date.now() - parsed.timestamp < 2 * 60 * 60 * 1000;
        const images = parsed.items.flatMap((it: Anime) => [it.coverUrl, it.bannerUrl]).filter(Boolean);
        preloadImages(images);
        return { items: parsed.items, isFresh };
      }
    } catch {
      // Ignore cache parse error
    }
    return null;
  }

  function saveCachedProviderCatalog(name: string, items: Anime[]) {
    try {
      if (items.length > 0) {
        localStorage.setItem(
          `any-watch:provider-catalog-v4:${name}`,
          JSON.stringify({ timestamp: Date.now(), items })
        );
        const images = items.flatMap((it) => [it.coverUrl, it.bannerUrl]).filter(Boolean);
        preloadImages(images);
      }
    } catch {
      // Ignore localStorage error
    }
  }

  function fetchProviderCatalog(sourceName: string, force = false) {
    const cached = getCachedProviderCatalog(sourceName);
    if (cached && cached.items.length > 0) {
      setProviderCatalog(cached.items);
      setProviderCatalogLoading(false);
      setProviderCatalogError(false);
      if (cached.isFresh && !force) {
        return;
      }
    } else {
      setProviderCatalogLoading(true);
      setProviderCatalogError(false);
    }

    api
      .getProviderCatalog(sourceName)
      .then((items) => {
        if (items.length > 0) {
          setProviderCatalog(items);
          saveCachedProviderCatalog(sourceName, items);
        }
        setProviderCatalogError(false);
        setProviderCatalogLoading(false);
      })
      .catch(() => {
        if (!cached || cached.items.length === 0) {
          setProviderCatalog([]);
          setProviderCatalogError(true);
        }
        setProviderCatalogLoading(false);
      });
  }

  useEffect(() => {
    if (!selectedSource?.name) {
      setProviderCatalog([]);
      setProviderCatalogError(false);
      return;
    }
    fetchProviderCatalog(selectedSource.name);
  }, [selectedSource?.name]);
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
              const isActive = selectedSource?.name === source.name || selectedAnime?.provider === source.name;
              const actionLabel = hasDirectResult ? "Results" : "Search";
              return (
                <button
                  key={source.name}
                  className={isActive ? "provider-chip active" : "provider-chip"}
                  aria-label={`${serverLabel(source.name, sources)}: ${actionLabel}`}
                  aria-pressed={isActive}
                  title={source.failureCode || option?.failureCode || undefined}
                  onClick={() => onProviderSourceSelect(source)}
                >
                  <i className={`health-dot ${source.status}`} />
                  <strong>{serverLabel(source.name, sources)}</strong>
                  <span>{actionLabel}</span>
                </button>
              );
            })}
            {!languageSources.length && (
              <span className="source-empty" role="status">
                No {languageGroup === "english" ? "English" : "Vietnamese"} servers available
              </span>
            )}
          </div>
        </div>
      </div>

      {query.trim().length < 2 && (
        <motion.section
          className="search-welcome provider-dashboard-welcome"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.18 }}
        >
          {/* Provider Live Catalog / Trending Shelf */}
          <div className="provider-dashboard-shelf">
            <div className="provider-shelf-heading">
              <div>
                <h3>
                  {providerCatalog.length > 0 && selectedSource
                    ? `Available on ${serverLabel(selectedSource.name, sources)}`
                    : selectedSource
                      ? `${serverLabel(selectedSource.name, sources)} Catalog`
                      : "Catalog Suggestions"}
                </h3>
                <p>
                  {providerCatalog.length > 0
                    ? `Anime, movies, and series available directly from ${selectedSource ? serverLabel(selectedSource.name, sources) : "this provider"}.`
                    : providerCatalogError
                      ? "This provider catalog is unavailable; showing general recommendations."
                      : "Browse titles or search for specific anime and films."}
                </p>
              </div>
              <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
                {providerCatalogError && (
                  <button
                    type="button"
                    className="provider-catalog-retry-btn"
                    onClick={() => selectedSource && fetchProviderCatalog(selectedSource.name)}
                  >
                    <RefreshCw size={13} />
                    <span>Retry</span>
                  </button>
                )}
                <small>{providerCatalog.length || fallbackSuggestions.length} titles</small>
              </div>
            </div>
            {providerCatalogLoading ? (
              <div className="provider-catalog-skeleton-grid" aria-label="Loading provider suggestions">
                {Array.from({ length: 8 }).map((_, i) => (
                  <div
                    key={i}
                    className="provider-catalog-card-skeleton"
                    style={{ animationDelay: `${i * 65}ms` }}
                  />
                ))}
              </div>
            ) : providerCatalog.length > 0 ? (
              <div className="provider-catalog-grid">
                {providerCatalog.map((item) => (
                  <article key={item.id} className="provider-catalog-card">
                    <button
                      type="button"
                      className="provider-catalog-thumb"
                      aria-label={`Open ${item.title}`}
                      title={item.title}
                      onClick={() => onOpenAnime(item)}
                    >
                      <img src={item.coverUrl || LOGO_SRC} alt="" loading="lazy" decoding="async" onError={useLogoFallback} />
                      <span className="provider-catalog-play"><Play size={22} fill="currentColor" /></span>
                      {item.totalEpisodes && (
                        <span className="provider-episodes-pill">{item.totalEpisodes} eps</span>
                      )}
                    </button>
                    <div className="provider-catalog-copy">
                      <strong title={item.title}>{item.title}</strong>
                      {item.synopsis && <p>{item.synopsis}</p>}
                    </div>
                  </article>
                ))}
              </div>
            ) : fallbackSuggestions.length > 0 ? (
              <div className="provider-catalog-grid provider-fallback-grid">
                {fallbackSuggestions.map((item) => (
                  <article key={item.catalogId} className="provider-catalog-card">
                    <button
                      type="button"
                      className="provider-catalog-thumb"
                      aria-label={`Explore provider availability for ${item.title}`}
                      title={item.title}
                      onClick={() => onOpenCatalog(item)}
                    >
                      <img src={item.coverUrl || LOGO_SRC} alt="" loading="lazy" decoding="async" onError={useLogoFallback} />
                      <span className="provider-catalog-play"><Search size={22} /></span>
                      <span className="provider-fallback-pill">General pick</span>
                    </button>
                    <div className="provider-catalog-copy">
                      <strong title={item.title}>{item.title}</strong>
                      <p>{item.format || "Title"}{item.seasonYear ? ` · ${item.seasonYear}` : ""}</p>
                    </div>
                  </article>
                ))}
              </div>
            ) : (
              <div className="provider-catalog-empty">
                <p>Search above or choose a topic to find something available to watch.</p>
              </div>
            )}
          </div>

          {/* Quick Search and Featured Tags on this Provider */}
          <div className="provider-discovery-shelf search-suggestions">
            <div className="provider-shelf-heading">
              <div>
                <h3>Quick Search & Topics</h3>
                <p>Popular series and catalogs on {selectedSource ? serverLabel(selectedSource.name, sources) : "this provider"}</p>
              </div>
            </div>
            <div className="provider-topic-chips" aria-label="Suggested search queries">
              {providerTopics.map((suggestion) => (
                <button
                  type="button"
                  key={suggestion}
                  className="provider-topic-btn"
                  onClick={() => {
                    onQueryChange(suggestion);
                    setTimeout(() => onSearch(suggestion), 0);
                  }}
                >
                  <Search size={14} />
                  {suggestion}
                </button>
              ))}
            </div>
          </div>
        </motion.section>
      )}

      {query.trim().length >= 2 && (
        <div className={`search-layout${mobilePreviewOpen ? " mobile-preview-open" : ""}`}>
          <aside className="search-results-pane">
            <div className="pane-title">
              <span>
                {selectedSource
                  ? `${serverLabel(selectedSource.name, sources)} Results`
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
                      <button
                        disabled={!selectedAnime}
                        title={selectedAnime ? undefined : "Choose an available provider before saving"}
                        onClick={() => selectedAnime && onToggleMyList(selectedAnime)}
                      >
                        {(() => {
                          const isFavorite = Boolean(selectedAnime && (
                            selectedAnime.isFavorite
                            || myList.some((item) => item.animeId === animeKey(selectedAnime.provider, selectedAnime.id))
                          ));
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

function parseYouTubeSynopsis(synopsis?: string | null) {
  if (!synopsis) return { author: "YouTube", meta: "", description: "" };
  const [firstLine = "", ...rest] = synopsis.split("\n");
  const parts = firstLine.split(" · ");
  const author = parts[0] || "YouTube";
  const meta = parts.slice(1).join(" · ");
  const description = rest.join("\n").trim();
  return { author, meta, description };
}

function YouTubeVideoCard({
  video,
  active,
  index = 0,
  onPlay,
  onSelect,
  onToggleMyList,
}: {
  video: Anime;
  active?: boolean;
  index?: number;
  onPlay: (video: Anime) => void;
  onSelect: (video: Anime) => void;
  onToggleMyList: (video: Anime) => void;
}) {
  const { author, meta } = parseYouTubeSynopsis(video.synopsis);
  const initial = (author || "Y").trim().charAt(0).toUpperCase();

  return (
    <motion.article
      key={animeKey(video.provider, video.id)}
      className={`youtube-shelf-card youtube-feed-card${active ? " active" : ""}`}
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.16, delay: Math.min(index * 0.015, 0.1) }}
    >
      <button
        type="button"
        className="youtube-shelf-card-thumb youtube-feed-thumbnail"
        onClick={() => onPlay(video)}
        aria-label={`Play ${video.title}`}
      >
        <img
          src={video.coverUrl || LOGO_SRC}
          alt=""
          loading="lazy"
          onError={useLogoFallback}
        />
        <span className="youtube-play-hover">
          <Play size={22} fill="currentColor" />
        </span>
      </button>
      <div className="youtube-shelf-card-meta youtube-feed-meta">
        <span className="youtube-channel-avatar small" aria-hidden="true">
          {initial}
        </span>
        <div className="youtube-shelf-card-copy youtube-feed-copy">
          <button
            type="button"
            className="youtube-shelf-card-title youtube-feed-title"
            onClick={() => onSelect(video)}
            title={video.title}
          >
            {video.title}
          </button>
          <span className="youtube-channel-name">{author}</span>
          {meta && <span className="youtube-video-stats">{meta}</span>}
          <div className="youtube-shelf-card-actions">
            <button
              type="button"
              className="primary"
              onClick={() => onPlay(video)}
              title="Play Video"
            >
              <Play size={13} fill="currentColor" /> Play
            </button>
            <button
              type="button"
              onClick={() => onSelect(video)}
              title="Open Watch Room"
            >
              <Tv size={13} /> Room
            </button>
            <button
              type="button"
              onClick={() => onToggleMyList(video)}
              title={video.isFavorite ? "In My List" : "Save to List"}
            >
              <Star size={13} fill={video.isFavorite ? "currentColor" : "none"} />
            </button>
          </div>
        </div>
      </div>
    </motion.article>
  );
}

function YouTubeShelf({
  title,
  subtitle,
  icon,
  iconClass,
  videos,
  loading,
  error,
  onSeeMore,
  onPlay,
  onSelect,
  onToggleMyList,
  onRetry,
}: {
  title: string;
  subtitle?: string;
  icon: ReactNode;
  iconClass?: string;
  videos: Anime[];
  loading: boolean;
  error?: string | null;
  onSeeMore?: () => void;
  onPlay: (video: Anime) => void;
  onSelect: (video: Anime) => void;
  onToggleMyList: (video: Anime) => void;
  onRetry?: () => void;
}) {
  const trackRef = useRef<HTMLDivElement | null>(null);

  const scroll = (direction: "left" | "right") => {
    if (!trackRef.current) return;
    const amount = direction === "left" ? -480 : 480;
    trackRef.current.scrollBy({ left: amount, behavior: "smooth" });
  };

  return (
    <section className="youtube-shelf-section">
      <div className="youtube-shelf-header">
        <div className="youtube-shelf-title-group">
          <div className={`youtube-shelf-icon ${iconClass ?? ""}`}>{icon}</div>
          <div className="youtube-shelf-titles">
            <h2>{title}</h2>
            {subtitle && <p>{subtitle}</p>}
          </div>
        </div>
        <div className="youtube-shelf-controls">
          <button
            type="button"
            className="youtube-shelf-arrow"
            onClick={() => scroll("left")}
            aria-label={`Scroll ${title} left`}
          >
            <ChevronLeft size={18} />
          </button>
          <button
            type="button"
            className="youtube-shelf-arrow"
            onClick={() => scroll("right")}
            aria-label={`Scroll ${title} right`}
          >
            <ChevronRight size={18} />
          </button>
          {onSeeMore && (
            <button
              type="button"
              className="youtube-shelf-see-more"
              onClick={onSeeMore}
              title={`View all ${title} in grid`}
            >
              <LayoutGrid size={15} />
              <span>See more</span>
            </button>
          )}
        </div>
      </div>

      {error ? (
        <div className="youtube-inline-error" role="alert">
          <AlertTriangle size={18} />
          <span>{error}</span>
          {onRetry && (
            <button type="button" onClick={onRetry}>
              Retry
            </button>
          )}
        </div>
      ) : loading && !videos.length ? (
        <div className="youtube-shelf-track">
          {Array.from({ length: 6 }).map((_, i) => (
            <div
              key={i}
              className="youtube-shelf-card youtube-card-skeleton"
              style={{ minWidth: "17.5rem" }}
            />
          ))}
        </div>
      ) : videos.length > 0 ? (
        <div className="youtube-shelf-track" ref={trackRef}>
          {videos.map((video, idx) => (
            <YouTubeVideoCard
              key={animeKey(video.provider, video.id)}
              video={video}
              index={idx}
              onPlay={onPlay}
              onSelect={onSelect}
              onToggleMyList={onToggleMyList}
            />
          ))}
        </div>
      ) : (
        <div className="provider-catalog-empty">No videos found for this section.</div>
      )}
    </section>
  );
}

function YouTubePage({
  query,
  results,
  selectedVideo,
  source,
  loading,
  topic,
  feedVideos,
  feedLoading,
  feedError,
  catalogs,
  viewMode = "dashboard",
  onViewModeChange,
  relatedVideos,
  relatedLoading,
  relatedError,
  continueWatching,
  onRemoveContinueWatching,
  onClearContinueWatching,
  watchMode,
  embedPlaying,
  resume,
  isFavorite,
  playerContext,
  autoSkip,
  onAutoSkipChange,
  onPlayPlayerEpisode,
  onPlayNextVideo,
  onClosePlayer,
  onQueryChange,
  onSearch,
  onTopicChange,
  onRetryFeed,
  onRetryRelated,
  onSelect,
  onPlay,
  onCloseWatch,
  onToggleMyList,
  onBack,
}: {
  query: string;
  results: Anime[];
  selectedVideo: Anime | null;
  source: Source | null;
  loading: boolean;
  topic: YouTubeTopic;
  feedVideos: Anime[];
  feedLoading: boolean;
  feedError: string | null;
  catalogs: YouTubeCatalogsData;
  viewMode?: "dashboard" | "grid";
  onViewModeChange?: (mode: "dashboard" | "grid") => void;
  relatedVideos: Anime[];
  relatedLoading: boolean;
  relatedError: string | null;
  continueWatching: WatchHistory[];
  onRemoveContinueWatching?: (animeId: string) => void;
  onClearContinueWatching?: () => void;
  watchMode: boolean;
  embedPlaying?: boolean;
  resume?: WatchHistory;
  isFavorite: boolean;
  playerContext: PlayerContext | null;
  autoSkip: boolean;
  onAutoSkipChange: (enabled: boolean) => void;
  onPlayPlayerEpisode: (episode: Episode) => Promise<void>;
  onPlayNextVideo?: (nextVideo: Anime) => void;
  onClosePlayer: () => void;
  onQueryChange: (query: string) => void;
  onSearch: () => void;
  onTopicChange: (topic: YouTubeTopic, viewMode?: "dashboard" | "grid") => void;
  onRetryFeed: () => void;
  onRetryRelated: () => void;
  onSelect: (video: Anime) => void;
  onPlay: (video: Anime, forceStream?: boolean) => void;
  onCloseWatch: () => void;
  onToggleMyList: (video: Anime) => void;
  onBack: () => void;
}) {
  const inputRef = useRef<HTMLInputElement | null>(null);
  const sourceReady = Boolean(source && source.status !== "unavailable");
  const [descExpanded, setDescExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const [theaterMode, setTheaterMode] = useState(false);
  const [autoplayNext, setAutoplayNext] = useState(true);
  const [countdownSeconds, setCountdownSeconds] = useState<number | null>(null);

  const youtubeContinueWatching = useMemo(
    () => continueWatching.filter((item) => item.provider === "Invidious" || item.provider === "YouTube"),
    [continueWatching],
  );

  const topics: YouTubeTopic[] = ["All", "Trending", "Music", "Films", "Anime", "Gaming", "News"];

  const copyShareLink = (videoId: string) => {
    const url = `${window.location.origin}/youtube?v=${encodeURIComponent(videoId)}`;
    void navigator.clipboard?.writeText(url);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  useEffect(() => {
    if (!watchMode) {
      inputRef.current?.focus();
    }
  }, [watchMode]);

  // Autoplay countdown timer when video ends
  useEffect(() => {
    if (countdownSeconds === null) return;
    if (countdownSeconds <= 0) {
      setCountdownSeconds(null);
      if (relatedVideos.length > 0 && onPlayNextVideo) {
        onPlayNextVideo(relatedVideos[0]);
      }
      return;
    }
    const timer = window.setTimeout(() => {
      setCountdownSeconds((prev) => (prev !== null ? prev - 1 : null));
    }, 1000);
    return () => window.clearTimeout(timer);
  }, [countdownSeconds, relatedVideos, onPlayNextVideo]);

  const handleVideoEnded = () => {
    if (autoplayNext && relatedVideos.length > 0) {
      setCountdownSeconds(5);
    }
  };

  return (
    <motion.section
      className={`youtube-page${watchMode ? " in-watch-mode" : ""}`}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
    >
      <header className="youtube-header">
        <IconButton label="Back" onClick={watchMode ? onCloseWatch : onBack}>
          <ArrowLeft size={21} />
        </IconButton>
        <div className="youtube-heading">
          <span className="youtube-heading-mark"><Play size={17} fill="currentColor" /></span>
          <div>
            <p className="eyebrow">Private video search & streaming</p>
            <h1>YouTube</h1>
          </div>
        </div>
        <span className={`youtube-source-status ${source?.status ?? "healthy"}`}>
          <i className={`health-dot ${source?.status ?? "healthy"}`} />
          {source ? `Invidious · ${providerStatusLabel(source)}` : "Invidious Source"}
        </span>
      </header>

      <form
        className="youtube-search"
        onSubmit={(event) => {
          event.preventDefault();
          onSearch();
        }}
      >
        <Search size={21} />
        <input
          ref={inputRef}
          type="search"
          value={query}
          aria-label="Search YouTube through Invidious"
          placeholder="Search YouTube, anime, films, music..."
          onChange={(event) => onQueryChange(event.target.value)}
        />
        {query.trim().length > 0 && (
          <button
            type="button"
            className="youtube-search-clear"
            aria-label="Clear search"
            onClick={() => onQueryChange("")}
          >
            <X size={16} />
          </button>
        )}
        {loading ? <Loader2 className="spin" size={20} /> : (
          <button type="submit" disabled={!sourceReady || query.trim().length < 2}>Search</button>
        )}
      </form>

      {!sourceReady && (
        <section className="youtube-unavailable" role="status">
          <AlertTriangle size={22} />
          <div>
            <strong>YouTube is not connected</strong>
            <p>Configure <code>ANY_WATCH_INVIDIOUS_URL</code> on the server. Public instances are used as fallback.</p>
          </div>
        </section>
      )}

      {sourceReady && !watchMode && (
        <div className="youtube-topic-bar" role="tablist" aria-label="YouTube categories">
          {topics.map((t) => (
            <button
              key={t}
              role="tab"
              aria-selected={topic === t && query.trim().length < 2}
              className={`youtube-topic-chip${topic === t && query.trim().length < 2 ? " active" : ""}`}
              onClick={() => onTopicChange(t, t === "All" ? "dashboard" : "grid")}
            >
              {t === "Trending" && <Flame size={13} style={{ marginRight: 4, display: "inline-block", verticalAlign: "middle" }} />}
              {t === "Music" && <Music size={13} style={{ marginRight: 4, display: "inline-block", verticalAlign: "middle" }} />}
              {t === "Films" && <Film size={13} style={{ marginRight: 4, display: "inline-block", verticalAlign: "middle" }} />}
              {t === "Anime" && <Sparkles size={13} style={{ marginRight: 4, display: "inline-block", verticalAlign: "middle" }} />}
              {t === "Gaming" && <Gamepad2 size={13} style={{ marginRight: 4, display: "inline-block", verticalAlign: "middle" }} />}
              {t === "News" && <Newspaper size={13} style={{ marginRight: 4, display: "inline-block", verticalAlign: "middle" }} />}
              {t}
            </button>
          ))}
        </div>
      )}

      {sourceReady && watchMode && selectedVideo && (
        <div className={`youtube-watch-room${theaterMode ? " theater-mode" : ""}`}>
          <div className="youtube-theater-pane">
            <div className="youtube-theater-frame">
            {playerContext ? (
              <VideoPlayer
                key={playerContext.playback.sessionId}
                context={playerContext}
                autoSkip={autoSkip}
                displayMode="inline"
                theaterMode={theaterMode}
                onToggleTheater={() => setTheaterMode(!theaterMode)}
                onPlayNext={relatedVideos.length > 0 && onPlayNextVideo ? () => (autoplayNext ? handleVideoEnded() : onPlayNextVideo(relatedVideos[0])) : undefined}
                onAutoSkipChange={onAutoSkipChange}
                onPlayEpisode={onPlayPlayerEpisode}
                onClose={onClosePlayer}
                onErrorFallback={() => onPlay(selectedVideo, false)}
              />
            ) : embedPlaying ? (
              <>
                <div className="youtube-embed-container">
                  <iframe
                    src={`https://www.youtube-nocookie.com/embed/${encodeURIComponent(selectedVideo.id)}?autoplay=1&rel=0&modestbranding=1&playsinline=1`}
                    title={selectedVideo.title}
                    allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
                    allowFullScreen
                    className="youtube-embed-iframe"
                  />
                </div>
                <div className="youtube-player-bottom-bar">
                  <button
                    type="button"
                    className="youtube-bottom-nav-btn"
                    onClick={onCloseWatch}
                    title="Back to feed"
                  >
                    <ArrowLeft size={16} />
                    <span>Back to feed</span>
                  </button>

                  <button
                    type="button"
                    className={`youtube-bottom-theater-btn ${theaterMode ? "active" : ""}`}
                    onClick={() => setTheaterMode(!theaterMode)}
                    title={theaterMode ? "Default view (T)" : "Theater mode (T)"}
                    aria-label="Toggle theater mode"
                  >
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <rect x="2" y="4" width="20" height="16" rx="2" />
                      <rect x="5" y="7" width="14" height="10" rx="1" fill={theaterMode ? "currentColor" : "none"} fillOpacity={0.25} />
                    </svg>
                    <span>{theaterMode ? "Default view" : "Theater mode"}</span>
                  </button>
                </div>
              </>
            ) : (() => {
              const progress = resume?.totalSeconds
                ? Math.min(100, Math.max(0, (resume.positionSeconds / resume.totalSeconds) * 100))
                : 0;
              return (
                <button
                  type="button"
                  className="youtube-theater-art"
                  onClick={() => onPlay(selectedVideo)}
                  aria-label={`${resume ? "Resume" : "Play"} ${selectedVideo.title}`}
                >
                  <img src={selectedVideo.bannerUrl || selectedVideo.coverUrl || LOGO_SRC} alt="" onError={useLogoFallback} />
                  <span className="youtube-theater-play"><Play size={36} fill="currentColor" /></span>
                  {progress > 0 && <i className="youtube-progress" style={{ "--youtube-progress": `${progress}%` } as React.CSSProperties} />}
                </button>
              );
            })()}

            {/* Autoplay Countdown Overlay */}
            {countdownSeconds !== null && relatedVideos.length > 0 && (
              <div className="player-autoplay-countdown">
                <span>Up next in {countdownSeconds}s: <strong>{relatedVideos[0].title}</strong></span>
                <button
                  type="button"
                  className="primary"
                  onClick={() => {
                    setCountdownSeconds(null);
                    if (onPlayNextVideo) onPlayNextVideo(relatedVideos[0]);
                  }}
                >
                  Play Now
                </button>
                <button
                  type="button"
                  className="secondary"
                  onClick={() => setCountdownSeconds(null)}
                >
                  Cancel
                </button>
              </div>
            )}
          </div>
          </div>

          <div className="youtube-watch-main">
            <h1 className="youtube-watch-title">{selectedVideo.title}</h1>

            {(() => {
              const { author, meta, description } = parseYouTubeSynopsis(selectedVideo.synopsis);
              const initial = (author || "Y").trim().charAt(0).toUpperCase();
              return (
                <>
                  <div className="youtube-channel-bar">
                    <div className="youtube-channel-profile">
                      <span className="youtube-channel-avatar" aria-hidden="true">{initial}</span>
                      <div className="youtube-channel-text">
                        <strong className="youtube-channel-name">{author}</strong>
                        <small className="youtube-channel-sub">Verified Channel · Official</small>
                      </div>
                      <button
                        type="button"
                        className={`youtube-subscribe-btn ${isFavorite ? "subscribed" : ""}`}
                        onClick={() => onToggleMyList(selectedVideo)}
                      >
                        {isFavorite ? (
                          <>
                            <Check size={14} />
                            <span>Subscribed</span>
                          </>
                        ) : (
                          <span>Subscribe</span>
                        )}
                      </button>
                    </div>

                    <div className="youtube-channel-actions">
                      <button className="primary play-now-pill" onClick={() => onPlay(selectedVideo)}>
                        <Play size={16} fill="currentColor" />
                        {resume ? `Resume (${formatTime(resume.positionSeconds)})` : "Play Now"}
                      </button>
                      <div className="youtube-like-dislike-pill">
                        <button
                          type="button"
                          className={`youtube-like-btn ${isFavorite ? "active" : ""}`}
                          onClick={() => onToggleMyList(selectedVideo)}
                          title="Like"
                        >
                          <ThumbsUp size={15} fill={isFavorite ? "currentColor" : "none"} />
                          <span>{isFavorite ? "Liked" : "Like"}</span>
                        </button>
                        <span className="youtube-pill-divider" />
                        <button type="button" className="youtube-dislike-btn" title="Dislike">
                          <ThumbsDown size={15} />
                        </button>
                      </div>
                      <button
                        type="button"
                        className="youtube-action-pill"
                        onClick={() => copyShareLink(selectedVideo.id)}
                        title="Copy video link"
                      >
                        {copied ? <Check size={15} /> : <Share2 size={15} />}
                        <span>{copied ? "Copied" : "Share"}</span>
                      </button>
                      <button
                        type="button"
                        className={`youtube-action-pill ${isFavorite ? "active" : ""}`}
                        onClick={() => onToggleMyList(selectedVideo)}
                        title="Save to Library"
                      >
                        <Bookmark size={15} fill={isFavorite ? "currentColor" : "none"} />
                        <span>{isFavorite ? "Saved" : "Save"}</span>
                      </button>
                    </div>
                  </div>

                  <div className={`youtube-desc-box${descExpanded ? " expanded" : ""}`}>
                    {meta && <div className="youtube-desc-meta">{meta}</div>}
                    <p className="youtube-desc-text">
                      {description ? plainDescription(description) : "No full description provided for this video."}
                    </p>
                    {description && description.length > 120 && (
                      <button
                        type="button"
                        className="youtube-desc-toggle"
                        onClick={() => setDescExpanded(!descExpanded)}
                      >
                        {descExpanded ? "Show less" : "...more"}
                      </button>
                    )}
                  </div>
                </>
              );
            })()}
          </div>

          <aside className="youtube-watch-sidebar">
            <div className="youtube-sidebar-header">
              <h3>Up Next & Related</h3>
              <div className="youtube-autoplay-toggle-wrapper">
                <button
                  type="button"
                  className={`youtube-autoplay-switch${autoplayNext ? " active" : ""}`}
                  onClick={() => setAutoplayNext(!autoplayNext)}
                  title="Toggle autoplay next video"
                >
                  Autoplay {autoplayNext ? "ON" : "OFF"}
                </button>
              </div>
            </div>
            {relatedLoading ? (
              <div className="youtube-related-skeleton">
                {Array.from({ length: 6 }).map((_, i) => (
                  <div key={i} className="youtube-related-skel-item" />
                ))}
              </div>
            ) : relatedError ? (
              <div className="youtube-inline-error" role="alert">
                <AlertTriangle size={18} />
                <span>{relatedError}</span>
                <button type="button" onClick={onRetryRelated}>Retry</button>
              </div>
            ) : relatedVideos.length > 0 ? (
              <div className="youtube-related-list">
                {relatedVideos.map((item) => {
                  const itemSynopsis = parseYouTubeSynopsis(item.synopsis);
                  return (
                    <button
                      key={item.id}
                      type="button"
                      className="youtube-related-card"
                      onClick={() => onPlay(item)}
                    >
                      <span className="youtube-related-thumb">
                        <img src={item.coverUrl || LOGO_SRC} alt="" loading="lazy" onError={useLogoFallback} />
                        <span className="youtube-related-play"><Play size={14} fill="currentColor" /></span>
                      </span>
                      <span className="youtube-related-copy">
                        <strong>{item.title}</strong>
                        <small>{itemSynopsis.author}</small>
                        {itemSynopsis.meta && <span>{itemSynopsis.meta}</span>}
                      </span>
                    </button>
                  );
                })}
              </div>
            ) : (
              <p className="empty-state">No related videos available.</p>
            )}
          </aside>
        </div>
      )}

      {sourceReady && !watchMode && query.trim().length >= 2 && (
        <section className="youtube-results-section" aria-busy={loading}>
          <div className="youtube-results-heading">
            <div>
              <p className="eyebrow">Search results</p>
              <h2>{query.trim()}</h2>
            </div>
            <strong>{results.length} videos</strong>
          </div>
          <div className="youtube-search-list">
            {results.map((video, index) => {
              const active = selectedVideo?.id === video.id;
              const { author, meta, description } = parseYouTubeSynopsis(video.synopsis);
              const initial = (author || "Y").trim().charAt(0).toUpperCase();
              return (
                <motion.article
                  key={animeKey(video.provider, video.id)}
                  className={`youtube-search-row${active ? " active" : ""}`}
                  initial={{ opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.18, delay: Math.min(index * 0.02, 0.12) }}
                >
                  <button
                    type="button"
                    className="youtube-search-thumbnail"
                    onClick={() => onPlay(video)}
                    aria-label={`Play ${video.title}`}
                  >
                    <img src={video.coverUrl || LOGO_SRC} alt="" loading="lazy" onError={useLogoFallback} />
                    <span className="youtube-play-hover">
                      <Play size={22} fill="currentColor" />
                    </span>
                  </button>
                  <div className="youtube-search-details">
                    <button
                      type="button"
                      className="youtube-search-title"
                      onClick={() => onSelect(video)}
                      title={video.title}
                    >
                      {video.title}
                    </button>
                    <div className="youtube-search-meta-row">
                      <span className="youtube-channel-avatar small" aria-hidden="true">{initial}</span>
                      <span className="youtube-channel-name">{author}</span>
                      {meta && <span className="youtube-video-stats">· {meta}</span>}
                    </div>
                    {description && (
                      <p className="youtube-search-desc">{plainDescription(description)}</p>
                    )}
                    <div className="youtube-search-actions">
                      <button className="primary" onClick={() => onPlay(video)}>
                        <Play size={15} fill="currentColor" />
                        Play
                      </button>
                      <button onClick={() => onToggleMyList(video)}>
                        <Star size={15} fill={video.isFavorite ? "currentColor" : "none"} />
                        {video.isFavorite ? "In My List" : "Save"}
                      </button>
                      <button onClick={() => onSelect(video)}>
                        Watch Room
                      </button>
                    </div>
                  </div>
                </motion.article>
              );
            })}
          </div>
          {!loading && !results.length && <EmptyPanel title="No videos found" compact />}
        </section>
      )}

      {sourceReady && !watchMode && query.trim().length < 2 && (
        <div className="youtube-home-feed">
          {topic === "All" ? (
            feedError ? (
              <section className="youtube-feed-section">
                <div className="youtube-inline-error" role="alert">
                  <AlertTriangle size={18} />
                  <span>{feedError}</span>
                  <button type="button" onClick={onRetryFeed}>Retry feed</button>
                </div>
              </section>
            ) : (
              /* Multi-Catalog Dashboard Overview with Horizontal Shelves */
              <div className="youtube-dashboard-shelves">
                {/* Continue Watching Shelf */}
                {youtubeContinueWatching.length > 0 && (
                  <section className="youtube-shelf-section">
                    <div className="youtube-shelf-header">
                      <div className="youtube-shelf-title-group">
                        <div className="youtube-shelf-icon history"><Clock size={20} /></div>
                        <div className="youtube-shelf-titles">
                          <h2>Continue Watching</h2>
                          <p>{youtubeContinueWatching.length} videos · Saved progress</p>
                        </div>
                      </div>
                      {onClearContinueWatching && (
                        <div className="youtube-shelf-controls">
                          <button
                            type="button"
                            className="youtube-clear-btn"
                            onClick={onClearContinueWatching}
                            title="Clear all YouTube watch history"
                          >
                            <Trash2 size={13} />
                            <span>Clear All</span>
                          </button>
                        </div>
                      )}
                    </div>
                    <div className="youtube-shelf-track">
                      {youtubeContinueWatching.map((item) => {
                        const progress = item.totalSeconds > 0
                          ? Math.min(100, (item.positionSeconds / item.totalSeconds) * 100)
                          : 0;
                        const videoObj: Anime = {
                          id: item.animeId.includes(":") ? item.animeId.split(":").slice(1).join(":") : item.animeId,
                          provider: "Invidious",
                          catalogId: item.catalogId ?? null,
                          title: item.title,
                          coverUrl: item.coverUrl,
                          bannerUrl: null,
                          language: "YouTube",
                          totalEpisodes: null,
                          synopsis: null,
                          isFavorite: false,
                        };
                        return (
                          <div key={item.animeId} className="youtube-shelf-card youtube-continue-card">
                            {onRemoveContinueWatching && (
                              <button
                                type="button"
                                className="youtube-continue-remove"
                                onClick={() => onRemoveContinueWatching(item.animeId)}
                                aria-label={`Remove ${item.title} from history`}
                                title="Remove from history"
                              >
                                <X size={13} />
                              </button>
                            )}
                            <button
                              type="button"
                              className="youtube-shelf-card-thumb"
                              onClick={() => onPlay(videoObj)}
                            >
                              <img src={item.coverUrl || LOGO_SRC} alt="" onError={useLogoFallback} />
                              <span className="youtube-play-hover"><Play size={20} fill="currentColor" /></span>
                              <i className="youtube-progress" style={{ "--youtube-progress": `${progress}%` } as React.CSSProperties} />
                              <span className="youtube-duration-pill">{formatTime(item.positionSeconds)}</span>
                            </button>
                            <div className="youtube-shelf-card-copy">
                              <button
                                type="button"
                                className="youtube-shelf-card-title"
                                onClick={() => onSelect(videoObj)}
                                title={item.title}
                              >
                                {item.title}
                              </button>
                              <small className="youtube-video-stats">Watched {Math.round(progress)}%</small>
                              <div className="youtube-shelf-card-actions">
                                <button type="button" className="primary" onClick={() => onPlay(videoObj)}>
                                  <Play size={12} fill="currentColor" /> Resume
                                </button>
                                <button type="button" onClick={() => onSelect(videoObj)}>
                                  <Tv size={12} /> Room
                                </button>
                              </div>
                            </div>
                          </div>
                        );
                      })}
                    </div>
                  </section>
                )}

                {/* 1. Trending Today Shelf */}
                <YouTubeShelf
                  title="Trending Today"
                  subtitle="Top trending videos on YouTube"
                  icon={<Flame size={20} />}
                  iconClass="trending"
                  videos={catalogs.trending.length > 0 ? catalogs.trending : feedVideos}
                  loading={Boolean(catalogs.loading.trending || feedLoading)}
                  error={catalogs.errors.trending}
                  onSeeMore={() => onTopicChange("Trending", "grid")}
                  onPlay={onPlay}
                  onSelect={onSelect}
                  onToggleMyList={onToggleMyList}
                  onRetry={onRetryFeed}
                />

                {/* 2. Music Hub Shelf */}
                <YouTubeShelf
                  title="Music Hub"
                  subtitle="Hot music videos, top tracks & lofi beats"
                  icon={<Music size={20} />}
                  iconClass="music"
                  videos={catalogs.music}
                  loading={Boolean(catalogs.loading.music)}
                  error={catalogs.errors.music}
                  onSeeMore={() => onTopicChange("Music", "grid")}
                  onPlay={onPlay}
                  onSelect={onSelect}
                  onToggleMyList={onToggleMyList}
                  onRetry={onRetryFeed}
                />

                {/* 3. Films & Cinema Shelf */}
                <YouTubeShelf
                  title="Films & Cinema"
                  subtitle="Trailers, short films & movie clips"
                  icon={<Film size={20} />}
                  iconClass="films"
                  videos={catalogs.films}
                  loading={Boolean(catalogs.loading.films)}
                  error={catalogs.errors.films}
                  onSeeMore={() => onTopicChange("Films", "grid")}
                  onPlay={onPlay}
                  onSelect={onSelect}
                  onToggleMyList={onToggleMyList}
                  onRetry={onRetryFeed}
                />

                {/* 4. Anime & Animation Shelf */}
                <YouTubeShelf
                  title="Anime & Animation"
                  subtitle="Openings, endings, trailers & anime shorts"
                  icon={<Sparkles size={20} />}
                  iconClass="anime"
                  videos={catalogs.anime}
                  loading={Boolean(catalogs.loading.anime)}
                  error={catalogs.errors.anime}
                  onSeeMore={() => onTopicChange("Anime", "grid")}
                  onPlay={onPlay}
                  onSelect={onSelect}
                  onToggleMyList={onToggleMyList}
                  onRetry={onRetryFeed}
                />

                {/* 5. Gaming & Esports Shelf */}
                <YouTubeShelf
                  title="Gaming & Esports"
                  subtitle="Top games, walkthroughs & highlights"
                  icon={<Gamepad2 size={20} />}
                  iconClass="gaming"
                  videos={catalogs.gaming}
                  loading={Boolean(catalogs.loading.gaming)}
                  error={catalogs.errors.gaming}
                  onSeeMore={() => onTopicChange("Gaming", "grid")}
                  onPlay={onPlay}
                  onSelect={onSelect}
                  onToggleMyList={onToggleMyList}
                  onRetry={onRetryFeed}
                />

                {/* 6. News & Tech Shelf */}
                <YouTubeShelf
                  title="News & Tech"
                  subtitle="Latest global news, tech reviews & updates"
                  icon={<Newspaper size={20} />}
                  iconClass="news"
                  videos={catalogs.news}
                  loading={Boolean(catalogs.loading.news)}
                  error={catalogs.errors.news}
                  onSeeMore={() => onTopicChange("News", "grid")}
                  onPlay={onPlay}
                  onSelect={onSelect}
                  onToggleMyList={onToggleMyList}
                  onRetry={onRetryFeed}
                />
              </div>
            )
          ) : (
            /* Specific Category View (Grid or Shelf) */
            <div className="youtube-grid-view">
              <div className="youtube-grid-header">
                <div className="youtube-grid-header-left">
                  <button
                    type="button"
                    className="youtube-grid-back-btn"
                    onClick={() => onTopicChange("All", "dashboard")}
                  >
                    <ArrowLeft size={16} /> All Catalogs
                  </button>
                  <div className="youtube-grid-title">
                    <h2>{topic} Catalog</h2>
                    <p>{feedVideos.length} videos available</p>
                  </div>
                </div>
                <div className="youtube-shelf-controls">
                  {onViewModeChange && (
                    <>
                      <button
                        type="button"
                        className={`youtube-shelf-see-more${viewMode === "grid" ? " active" : ""}`}
                        onClick={() => onViewModeChange("grid")}
                        title="Grid View"
                      >
                        <Grid size={15} /> <span>Grid</span>
                      </button>
                      <button
                        type="button"
                        className={`youtube-shelf-see-more${viewMode === "dashboard" ? " active" : ""}`}
                        onClick={() => onViewModeChange("dashboard")}
                        title="Shelf View"
                      >
                        <List size={15} /> <span>Shelf</span>
                      </button>
                    </>
                  )}
                  <button
                    type="button"
                    className="youtube-shelf-arrow"
                    onClick={onRetryFeed}
                    title="Refresh Feed"
                  >
                    <RefreshCw size={15} />
                  </button>
                </div>
              </div>

              {feedError ? (
                <div className="youtube-inline-error" role="alert">
                  <AlertTriangle size={18} />
                  <span>{feedError}</span>
                  <button type="button" onClick={onRetryFeed}>Retry feed</button>
                </div>
              ) : feedLoading && !feedVideos.length ? (
                <div className="youtube-catalog-grid">
                  {Array.from({ length: 12 }).map((_, i) => (
                    <div key={i} className="youtube-shelf-card youtube-card-skeleton" style={{ aspectRatio: "16/11" }} />
                  ))}
                </div>
              ) : feedVideos.length > 0 ? (
                viewMode === "grid" ? (
                  <div className="youtube-catalog-grid">
                    {feedVideos.map((video, index) => (
                      <YouTubeVideoCard
                        key={animeKey(video.provider, video.id)}
                        video={video}
                        index={index}
                        active={selectedVideo?.id === video.id}
                        onPlay={onPlay}
                        onSelect={onSelect}
                        onToggleMyList={onToggleMyList}
                      />
                    ))}
                  </div>
                ) : (
                  <div className="youtube-shelf-track">
                    {feedVideos.map((video, index) => (
                      <YouTubeVideoCard
                        key={animeKey(video.provider, video.id)}
                        video={video}
                        index={index}
                        active={selectedVideo?.id === video.id}
                        onPlay={onPlay}
                        onSelect={onSelect}
                        onToggleMyList={onToggleMyList}
                      />
                    ))}
                  </div>
                )
              ) : (
                <EmptyPanel title="No videos available in this category" compact />
              )}
            </div>
          )}
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
  const [tasks, setTasks] = useState<TorrentTask[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingTasks, setLoadingTasks] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [role, setRole] = useState("user");
  const [creating, setCreating] = useState(false);
  const [approvingTaskIds, setApprovingTaskIds] = useState<Set<string>>(new Set());
  const [deletingTaskIds, setDeletingTaskIds] = useState<Set<string>>(new Set());
  const [rejectingTask, setRejectingTask] = useState<TorrentTask | null>(null);
  const [rejectReason, setRejectReason] = useState("");
  const [rejectSubmitting, setRejectSubmitting] = useState(false);

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

  async function loadTasks() {
    setLoadingTasks(true);
    try {
      setTasks(await api.listTorrentTasks());
    } catch {
      // ignore
    } finally {
      setLoadingTasks(false);
    }
  }

  useEffect(() => {
    void loadUsers();
    void loadTasks();
  }, []);

  const handleApproveTask = async (id: string) => {
    setApprovingTaskIds((current) => new Set([...current, id]));
    try {
      const updated = await api.approveTorrentTask(id);
      setTasks((current) => current.map((t) => (t.id === id ? updated : t)));
    } catch (err: unknown) {
      setMessage(err instanceof Error ? err.message : "Failed to approve media task.");
    } finally {
      setApprovingTaskIds((current) => {
        const next = new Set(current);
        next.delete(id);
        return next;
      });
    }
  };

  const handleConfirmReject = async () => {
    if (!rejectingTask) return;
    setRejectSubmitting(true);
    try {
      const reason = rejectReason.trim() || "Rejected by admin: Storage quota or release suitability";
      const updated = await api.rejectTorrentTask(rejectingTask.id, reason);
      setTasks((current) => current.map((t) => (t.id === rejectingTask.id ? updated : t)));
      setRejectingTask(null);
      setRejectReason("");
    } catch (err: unknown) {
      setMessage(err instanceof Error ? err.message : "Failed to reject media task.");
    } finally {
      setRejectSubmitting(false);
    }
  };

  const handleDeleteTask = async (id: string) => {
    setDeletingTaskIds((current) => new Set([...current, id]));
    try {
      await api.deleteTorrentTask(id);
      setTasks((current) => current.filter((t) => t.id !== id));
    } catch (err: unknown) {
      setMessage(err instanceof Error ? err.message : "Failed to delete task.");
    } finally {
      setDeletingTaskIds((current) => {
        const next = new Set(current);
        next.delete(id);
        return next;
      });
    }
  };

  const pendingTasks = useMemo(() => tasks.filter((t) => t.status.type === "pending_approval"), [tasks]);

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
        <div><p className="eyebrow">Administrator</p><h1>People & Library Access</h1><p>Manage viewer accounts, review requests, and approve shared storage films.</p></div>
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

        {/* Film Requests Moderation Card in the Same Admin View */}
        <section className="admin-users-card admin-requests-card" style={{ gridColumn: "1 / -1", marginTop: "1rem" }}>
          <div className="admin-card-heading">
            <div>
              <p className="eyebrow">Library Moderation</p>
              <h2>Pending Film Requests ({pendingTasks.length})</h2>
            </div>
            <Film size={22} />
          </div>
          {loadingTasks ? (
            <div className="admin-loading" style={{ minHeight: "6rem" }}><Loader2 className="spin" /> Loading film requests…</div>
          ) : pendingTasks.length > 0 ? (
            <div className="torrent-tasks-list" style={{ marginTop: "0.85rem" }}>
              {pendingTasks.map((task) => (
                <TorrentTaskItem
                  key={task.id}
                  task={task}
                  userRole="admin"
                  isDeleting={deletingTaskIds.has(task.id)}
                  isApproving={approvingTaskIds.has(task.id)}
                  onApprove={(id) => void handleApproveTask(id)}
                  onReject={(id) => {
                    const t = tasks.find((item) => item.id === id);
                    if (t) setRejectingTask(t);
                  }}
                  onRequestReject={(t) => setRejectingTask(t)}
                  onDelete={(id) => void handleDeleteTask(id)}
                />
              ))}
            </div>
          ) : (
            <p style={{ color: "var(--color-muted)", fontSize: "var(--text-sm)", padding: "0.5rem 0", margin: 0 }}>
              No pending film requests waiting for approval. All requests are currently up to date!
            </p>
          )}
        </section>
      </div>

      {/* Admin Rejection Modal */}
      {rejectingTask && (
        <div className="login-modal-overlay" onClick={() => setRejectingTask(null)}>
          <div
            className="login-modal-dialog"
            onClick={(e) => e.stopPropagation()}
            style={{ maxWidth: "28rem", padding: "1.5rem" }}
          >
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: "1rem" }}>
              <div style={{ display: "flex", alignItems: "center", gap: "0.5rem" }}>
                <AlertTriangle size={18} color="#f87171" />
                <h3 style={{ margin: 0, fontSize: "var(--text-lg)" }}>Reject Film Request</h3>
              </div>
              <button
                type="button"
                className="torrent-search-clear"
                onClick={() => setRejectingTask(null)}
                aria-label="Close dialog"
              >
                <X size={16} />
              </button>
            </div>

            <p style={{ margin: "0 0 1rem", fontSize: "var(--text-sm)", color: "var(--color-muted)" }}>
              Specify the rejection reason for "<strong>{rejectingTask.title}</strong>". The requester will see this note in their Request History.
            </p>

            <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem", marginBottom: "1rem" }}>
              <button
                type="button"
                className="storage-film-btn"
                onClick={() => setRejectReason("Insufficient storage quota (Homelab storage full)")}
                style={{ textAlign: "left", justifyContent: "flex-start" }}
              >
                1. Insufficient storage quota (100GB quota reached)
              </button>
              <button
                type="button"
                className="storage-film-btn"
                onClick={() => setRejectReason("Duplicate release already available in Shared Storage")}
                style={{ textAlign: "left", justifyContent: "flex-start" }}
              >
                2. Duplicate release already available in storage
              </button>
              <button
                type="button"
                className="storage-film-btn"
                onClick={() => setRejectReason("Low seeder health / slow torrent source")}
                style={{ textAlign: "left", justifyContent: "flex-start" }}
              >
                3. Low seeder health / dead source
              </button>
              <button
                type="button"
                className="storage-film-btn"
                onClick={() => setRejectReason("Low audio/video quality or unsupported release format")}
                style={{ textAlign: "left", justifyContent: "flex-start" }}
              >
                4. Low audio/video quality release
              </button>
            </div>

            <label style={{ display: "block", fontSize: "var(--text-xs)", marginBottom: "0.35rem", color: "var(--color-muted)" }}>
              Custom Reason Note:
            </label>
            <textarea
              rows={3}
              value={rejectReason}
              onChange={(e) => setRejectReason(e.target.value)}
              placeholder="e.g. Please search for a 1080p release instead of 4K 80GB..."
              style={{
                width: "100%",
                padding: "0.6rem",
                borderRadius: "var(--radius-input)",
                background: "var(--color-paper-2)",
                border: "1px solid var(--color-glass-hairline)",
                color: "var(--color-ink)",
                resize: "vertical",
                fontSize: "var(--text-sm)",
                marginBottom: "1.25rem",
              }}
            />

            <div style={{ display: "flex", gap: "0.75rem", justifyContent: "flex-end" }}>
              <button
                type="button"
                className="storage-film-btn"
                onClick={() => setRejectingTask(null)}
                disabled={rejectSubmitting}
              >
                Cancel
              </button>
              <button
                type="button"
                className="storage-film-btn"
                onClick={handleConfirmReject}
                disabled={rejectSubmitting}
                style={{ background: "rgba(239, 68, 68, 0.2)", borderColor: "#ef4444", color: "#f87171", fontWeight: 700 }}
              >
                {rejectSubmitting ? <Loader2 size={14} className="spin" /> : <X size={14} />}
                <span>{rejectSubmitting ? "Rejecting..." : "Confirm Rejection"}</span>
              </button>
            </div>
          </div>
        </div>
      )}
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
  const [role, setRole] = useState<"admin" | "user">(user.role);
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
                      <motion.article
                        className={`episode-list-row${episode.thumbnail ? " has-thumbnail" : ""}${isResume ? " watched" : ""}${highlighted ? " highlighted" : ""}`}
                        key={episode.id}
                        data-episode-number={episode.number}
                        whileHover={{ y: -2 }}
                      >
                        <button
                          type="button"
                          className="episode-open-button"
                          aria-label={`${isResume ? "Resume" : "Play"} Episode ${episode.number}`}
                          onClick={() => onPlay(episode, isResume ? resumeHistory?.positionSeconds ?? 0 : 0)}
                        >
                          <span className="episode-thumb">
                            {episode.thumbnail ? <img src={episode.thumbnail} alt="" loading="lazy" onError={useLogoFallback} /> : <Play size={18} />}
                          </span>
                          <span className="episode-row-copy">
                            <strong>Episode {episode.number}</strong>
                            <small>{episodeTitleDetail(episode.title, episode.number) || "Ready to play"}</small>
                          </span>
                          {isResume && <span className="episode-resume-pill">Resume</span>}
                          <Play className="episode-play-icon" size={18} fill="currentColor" />
                        </button>
                        <button
                          className={`episode-download-button ${downloadState?.status || "idle"}`}
                          disabled={downloadBusy}
                          aria-label={`Download Episode ${episode.number}`}
                          title={downloadState?.message || `Download Episode ${episode.number}`}
                          style={{ "--download-progress": `${downloadState?.progress ?? 0}%` } as React.CSSProperties}
                          onClick={() => onDownload(episode)}
                        >
                          {downloadBusy ? <Loader2 className="spin" size={17} /> : downloadState?.status === "complete" ? <Check size={17} /> : <Download size={17} />}
                        </button>
                      </motion.article>
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
  displayMode = "overlay",
  theaterMode,
  onToggleTheater,
  onPlayNext,
  onAutoSkipChange,
  onPlayEpisode,
  onClose,
  onErrorFallback,
}: {
  context: PlayerContext;
  autoSkip: boolean;
  displayMode?: "overlay" | "inline";
  theaterMode?: boolean;
  onToggleTheater?: () => void;
  onPlayNext?: () => void;
  onAutoSkipChange: (enabled: boolean) => void;
  onPlayEpisode: (episode: Episode) => Promise<void>;
  onClose: () => void;
  onErrorFallback?: () => void;
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
  const [playbackSpeed, setPlaybackSpeed] = useState(1);
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
    const skipNumber = context.episode.aniskipEpisodeNumber ?? context.episode.number;
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
  }, [context.anime.catalogId, context.episode.id, context.episode.aniskipEpisodeNumber, context.episode.number]);

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
      void video.play().catch(() => {
        if (!disposed) setError("Playback is ready. Press Play to start on this device.");
      });
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
        player.updateSettings({
          streaming: {
            abr: { initialBitrate: { video: 2500 } },
            buffer: {
              fastSwitchEnabled: true,
              initialBufferLevel: 4,
              bufferTimeDefault: 12,
              bufferTimeAtTopQuality: 30,
            },
          },
        });
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
      const canNativeHls = Boolean(video.canPlayType("application/vnd.apple.mpegurl"));
      const startNativeHls = () => {
        if (!canNativeHls) {
          setError("This browser cannot play HLS streams.");
          return;
        }
        video.src = context.playback.playbackUrl;
        video.addEventListener("loadedmetadata", startPlayback, { once: true });
        video.load();
      };

      if (prefersNativeHls() && canNativeHls) {
        startNativeHls();
      } else {
        void import("hls.js").then(({ default: HlsRuntime }) => {
          if (disposed) return;
          if (!HlsRuntime.isSupported()) {
            startNativeHls();
            return;
          }

          const hls = new HlsRuntime({
            capLevelToPlayerSize: false,
            enableWorker: true,
            startFragPrefetch: true,
            lowLatencyMode: false,
            abrEwmaDefaultEstimate: 2_500_000,
          });
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
          if (canNativeHls) {
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

  useEffect(() => () => {
    void saveProgress(true).catch(() => undefined);
  }, [context.playback.sessionId]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      if (target?.tagName === "INPUT" || target?.tagName === "SELECT" || target?.tagName === "TEXTAREA") return;

      if (event.key === " " || event.key.toLowerCase() === "k") {
        event.preventDefault();
        togglePlay();
      } else if (event.key === "ArrowLeft" || event.key.toLowerCase() === "j") {
        event.preventDefault();
        seekBy(-10);
      } else if (event.key === "ArrowRight" || event.key.toLowerCase() === "l") {
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
      } else if (event.key.toLowerCase() === "t") {
        if (onToggleTheater) {
          event.preventDefault();
          onToggleTheater();
        }
      } else if (event.key === ">" || (event.shiftKey && event.key === ".")) {
        event.preventDefault();
        const speeds = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2];
        const next = speeds.find((s) => s > playbackSpeed) ?? 2;
        changePlaybackSpeed(next);
      } else if (event.key === "<" || (event.shiftKey && event.key === ",")) {
        event.preventDefault();
        const speeds = [0.25, 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2];
        const next = [...speeds].reverse().find((s) => s < playbackSpeed) ?? 0.25;
        changePlaybackSpeed(next);
      } else if (event.key >= "0" && event.key <= "9" && duration > 0) {
        event.preventDefault();
        const pct = (Number(event.key) * 10) / 100;
        if (videoRef.current) {
          videoRef.current.currentTime = duration * pct;
          setCurrentTime(videoRef.current.currentTime);
          showSkipFeedback(`${Number(event.key) * 10}%`);
        }
      } else if (event.key === "Escape") {
        event.preventDefault();
        void closePlayer();
      } else if (event.key === "[") {
        event.preventDefault();
        if (previousEpisode) void changeEpisode(previousEpisode);
      } else if (event.key === "]") {
        event.preventDefault();
        if (nextEpisode) {
          void changeEpisode(nextEpisode);
        } else if (onPlayNext) {
          onPlayNext();
        }
      }
      revealControls();
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [volume, muted, isPlaying, previousEpisode?.id, nextEpisode?.id, switchingEpisode, playbackSpeed, duration, onPlayNext, onToggleTheater]);

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

  function changePlaybackSpeed(nextSpeed: number) {
    setPlaybackSpeed(nextSpeed);
    if (videoRef.current) {
      videoRef.current.playbackRate = nextSpeed;
    }
    showSkipFeedback(`${nextSpeed}x Speed`);
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
    const pos = Math.floor(video.currentTime || 0);
    const dur = Math.floor(Number.isFinite(video.duration) ? video.duration : 0);

    if (context.anime.provider === "Invidious" || context.anime.provider === "YouTube") {
      saveCachedYouTubeMetadata({
        id: context.anime.id,
        provider: context.anime.provider,
        title: context.anime.title,
        coverUrl: context.anime.coverUrl,
        bannerUrl: context.anime.bannerUrl,
        duration: dur,
        lastPosition: pos,
        lastWatchedAt: now,
      });
    }

    await api.saveProgress({
      animeId: animeKey(context.anime.provider, context.anime.id),
      catalogId: context.anime.catalogId ?? null,
      provider: context.anime.provider,
      title: context.anime.title,
      coverUrl: context.anime.coverUrl,
      episodeNumber: context.episode.number,
      episodeTitle: context.episode.title,
      positionSeconds: pos,
      totalSeconds: dur,
    }).catch(() => undefined);
  }

  function togglePlay() {
    const video = videoRef.current;
    if (!video) return;
    if (video.paused) {
      void video.play()
        .then(() => setError(null))
        .catch(() => setError("Playback could not start on this device."));
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
    await saveProgress(true).catch(() => undefined);
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

  const playerClassName = [
    "player-overlay",
    "video-player",
    displayMode === "inline" ? "inline-player" : "",
    showControls ? "controls-visible" : "",
  ].filter(Boolean).join(" ");

  return (
    <motion.div
      className={playerClassName}
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
        playsInline
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

      {displayMode !== "inline" && (
        <div className="player-volume-dock overlay-volume-dock">
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
      )}

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
          onClick={() => {
            if (nextEpisode) {
              void changeEpisode(nextEpisode);
            } else if (onPlayNext) {
              onPlayNext();
            }
          }}
          disabled={(!nextEpisode && !onPlayNext) || switchingEpisode}
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
            {onErrorFallback && (
              <button
                type="button"
                className="player-fallback-switch-btn"
                onClick={onErrorFallback}
              >
                Switch to Official YouTube Player
              </button>
            )}
          </div>
        )}

        {/* Scrubber timeline bar above controls, matching YouTube desktop */}
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

        <div className="player-control-row">
          {displayMode === "inline" ? (
            <div className="player-left-group">
              <button className="player-control-btn play-btn" onClick={togglePlay} aria-label={isPlaying ? "Pause" : "Play"} title={isPlaying ? "Pause (k)" : "Play (k)"}>
                {isPlaying ? <Pause size={19} /> : <Play size={19} />}
              </button>

              {onPlayNext && (
                <button
                  className="player-control-btn next-btn"
                  onClick={onPlayNext}
                  aria-label="Next video"
                  title="Next video (Shift+N)"
                >
                  <SkipForward size={18} />
                </button>
              )}

              <div className="player-volume-dock">
                <button onClick={toggleMute} aria-label={muted ? "Unmute" : "Mute"} title={muted ? "Unmute (m)" : "Mute (m)"}>
                  {muted || volume === 0 ? <VolumeX size={19} /> : <Volume2 size={19} />}
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

              <div className="player-time-display">
                <span>{formatTime(currentTime)}</span>
                <span className="player-time-sep">/</span>
                <span>{formatTime(duration)}</span>
              </div>
            </div>
          ) : (
            <div className="player-now-playing">
              <span>{context.anime.provider}</span>
              <strong>{context.anime.title}</strong>
              <small>{episodeLabel(context.episode.number, context.episode.title)}</small>
            </div>
          )}

          {displayMode === "inline" && (
            <div className="player-now-playing">
              <span>{context.anime.provider}</span>
              <strong>{context.anime.title}</strong>
              <small>{episodeLabel(context.episode.number, context.episode.title)}</small>
            </div>
          )}

          <div className="player-utility-pill">
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
            {(displayMode === "inline" || context.anime.provider === "Invidious") && (
              <label title="Playback speed">
                <span>Speed</span>
                <select value={playbackSpeed} onChange={(event) => changePlaybackSpeed(Number(event.target.value))}>
                  <option value={0.25}>0.25x</option>
                  <option value={0.5}>0.5x</option>
                  <option value={0.75}>0.75x</option>
                  <option value={1}>1x</option>
                  <option value={1.25}>1.25x</option>
                  <option value={1.5}>1.5x</option>
                  <option value={1.75}>1.75x</option>
                  <option value={2}>2x</option>
                </select>
              </label>
            )}
            <label title="Video quality">
              <span>Quality</span>
              <select value={quality} onChange={(event) => changeQuality(event.target.value)} disabled={(!streamIsHls && !streamIsDash) || !levels.length}>
                <option value="auto">Auto</option>
                {levels.map((level) => <option value={String(level.index)} key={level.index}>{level.label}</option>)}
              </select>
            </label>
            <button onClick={() => void togglePictureInPicture()} aria-label="Picture in Picture" title="Picture in Picture (i)">
              <PictureInPicture2 size={18} />
            </button>
            {onToggleTheater && (
              <button
                type="button"
                onClick={onToggleTheater}
                aria-label="Toggle theater mode"
                title={theaterMode ? "Default view (t)" : "Theater mode (t)"}
                className={`player-theater-btn ${theaterMode ? "active" : ""}`}
              >
                <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <rect x="2" y="4" width="20" height="16" rx="2" />
                  <rect x="5" y="7" width="14" height="10" rx="1" fill={theaterMode ? "currentColor" : "none"} fillOpacity={0.25} />
                </svg>
              </button>
            )}
            <button onClick={() => void toggleFullscreen()} aria-label="Toggle fullscreen" title="Toggle fullscreen (F)">
              <Maximize2 size={18} />
            </button>
          </div>
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
    if (
      saved === "obsidian" ||
      saved === "oled" ||
      saved === "ember" ||
      saved === "crimson" ||
      saved === "tokyo" ||
      saved === "cyberpunk" ||
      saved === "emerald" ||
      saved === "amethyst" ||
      saved === "sunset" ||
      saved === "nordic" ||
      saved === "system"
    ) {
      return saved;
    }
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
    if (
      saved === "manrope" ||
      saved === "noto" ||
      saved === "jakarta" ||
      saved === "outfit" ||
      saved === "vietnam" ||
      saved === "mono" ||
      saved === "system"
    ) {
      return saved;
    }
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
