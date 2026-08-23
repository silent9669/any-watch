import type { Anime, WatchHistory } from "./types";

const GUEST_HISTORY_KEY = "any-watch:guest-watch-history";
const YOUTUBE_META_CACHE_KEY = "any-watch:youtube-metadata-cache";
const MAX_GUEST_HISTORY = 100;
const MAX_META_CACHE = 250;

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

export function loadGuestWatchHistory(): WatchHistory[] {
  try {
    const raw = localStorage.getItem(GUEST_HISTORY_KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (Array.isArray(parsed)) {
      return parsed.filter(
        (item) => item && typeof item.animeId === "string" && typeof item.title === "string",
      );
    }
    return [];
  } catch {
    return [];
  }
}

export function saveGuestWatchProgress(
  input: {
    animeId: string;
    catalogId?: number | null;
    provider: string;
    title: string;
    coverUrl: string;
    episodeNumber?: number;
    episodeTitle?: string | null;
    positionSeconds: number;
    totalSeconds: number;
    synopsis?: string | null;
    bannerUrl?: string | null;
  },
): WatchHistory[] {
  try {
    const current = loadGuestWatchHistory();
    const now = new Date().toISOString();
    const updatedItem: WatchHistory = {
      animeId: input.animeId,
      catalogId: input.catalogId ?? null,
      provider: input.provider,
      title: input.title,
      coverUrl: input.coverUrl,
      episodeNumber: input.episodeNumber ?? 1,
      episodeTitle: input.episodeTitle ?? null,
      positionSeconds: Math.floor(input.positionSeconds || 0),
      totalSeconds: Math.floor(input.totalSeconds || 0),
      updatedAt: now,
    };

    const next = [
      updatedItem,
      ...current.filter((item) => item.animeId !== input.animeId),
    ].slice(0, MAX_GUEST_HISTORY);

    localStorage.setItem(GUEST_HISTORY_KEY, JSON.stringify(next));

    // Also cache video metadata for fast instant re-querying
    const rawId = input.animeId.includes(":")
      ? input.animeId.split(":").slice(1).join(":")
      : input.animeId;
    saveCachedYouTubeMetadata({
      id: rawId,
      provider: input.provider,
      title: input.title,
      coverUrl: input.coverUrl,
      bannerUrl: input.bannerUrl ?? null,
      synopsis: input.synopsis ?? null,
      duration: input.totalSeconds,
      lastPosition: input.positionSeconds,
      lastWatchedAt: Date.now(),
    });

    return next;
  } catch {
    return [];
  }
}

export function removeGuestWatchHistoryItem(animeId: string): WatchHistory[] {
  try {
    const current = loadGuestWatchHistory();
    const next = current.filter((item) => item.animeId !== animeId);
    localStorage.setItem(GUEST_HISTORY_KEY, JSON.stringify(next));
    return next;
  } catch {
    return [];
  }
}

export function clearAllGuestWatchHistory(): void {
  try {
    localStorage.removeItem(GUEST_HISTORY_KEY);
  } catch {
    // Ignore storage errors
  }
}

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
    const cache = loadAllAllCachedMetadata();
    return cache[videoId] ?? null;
  } catch {
    return null;
  }
}

function loadAllAllCachedMetadata(): Record<string, CachedYouTubeMetadata> {
  return loadAllCachedYouTubeMetadata();
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
