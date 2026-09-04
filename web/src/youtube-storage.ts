import type { Anime } from "./types";

const YOUTUBE_META_CACHE_KEY = "any-watch:youtube-metadata-cache";
const YOUTUBE_CATALOG_CACHE_KEY = "any-watch:youtube-catalog-v2";
const MAX_META_CACHE = 250;
const CATALOG_CACHE_TTL_MS = 24 * 60 * 60 * 1000; // 24 hours

export type CachedYouTubeMetadata = {
  id: string;
  provider: string;
  title: string;
  coverUrl: string;
  bannerUrl?: string | null;
  author?: string;
  synopsis?: string | null;
  duration?: number;
  lastPosition?: number;
  lastWatchedAt?: number;
};

export function loadAllCachedYouTubeMetadata(): Record<string, CachedYouTubeMetadata> {
  try {
    const raw = localStorage.getItem(YOUTUBE_META_CACHE_KEY);
    if (!raw) return {};
    return JSON.parse(raw) ?? {};
  } catch {
    return {};
  }
}

export function getCachedYouTubeMetadata(videoId: string): CachedYouTubeMetadata | null {
  try {
    const cache = loadAllCachedYouTubeMetadata();
    return cache[videoId] ?? null;
  } catch {
    return null;
  }
}

export function saveCachedYouTubeMetadata(meta: CachedYouTubeMetadata): void {
  try {
    const cache = loadAllCachedYouTubeMetadata();
    cache[meta.id] = {
      ...cache[meta.id],
      ...meta,
      lastWatchedAt: meta.lastWatchedAt || Date.now(),
    };
    const keys = Object.keys(cache);
    if (keys.length > MAX_META_CACHE) {
      // Evict oldest entries
      const sortedKeys = keys.sort((a, b) => (cache[a].lastWatchedAt || 0) - (cache[b].lastWatchedAt || 0));
      for (const oldestKey of sortedKeys.slice(0, keys.length - MAX_META_CACHE)) {
        delete cache[oldestKey];
      }
    }
    localStorage.setItem(YOUTUBE_META_CACHE_KEY, JSON.stringify(cache));
  } catch {
    // Ignore storage errors
  }
}

export function loadCachedYouTubeCatalog(topic = "all"): { items: Anime[]; isFresh: boolean } | null {
  try {
    const raw = localStorage.getItem(`${YOUTUBE_CATALOG_CACHE_KEY}:${topic.toLowerCase()}`);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (parsed && Array.isArray(parsed.items) && typeof parsed.timestamp === "number") {
      const isFresh = Date.now() - parsed.timestamp < CATALOG_CACHE_TTL_MS;
      return { items: parsed.items, isFresh };
    }
    return null;
  } catch {
    return null;
  }
}

export function saveCachedYouTubeCatalog(topic = "all", items: Anime[]): void {
  try {
    if (items.length === 0) return;
    localStorage.setItem(
      `${YOUTUBE_CATALOG_CACHE_KEY}:${topic.toLowerCase()}`,
      JSON.stringify({ timestamp: Date.now(), items })
    );
  } catch {
    // Ignore storage errors
  }
}

export function cachedMetaToAnime(meta: CachedYouTubeMetadata, isFavorite = false): Anime {
  return {
    id: meta.id,
    provider: meta.provider || "Invidious",
    title: meta.title,
    coverUrl: meta.coverUrl,
    bannerUrl: meta.bannerUrl ?? meta.coverUrl,
    language: "YouTube",
    totalEpisodes: null,
    synopsis: meta.synopsis ?? (meta.author ? meta.author : null),
    isFavorite,
  };
}
