import type {
  Anime,
  AnimeDetails,
  CatalogAnime,
  CatalogFilters,
  CatalogPage,
  DiscoveryCatalog,
  DownloadEvent,
  DownloadResult,
  Episode,
  Favorite,
  ManagedUser,
  Playback,
  ProviderAvailability,
  SessionUser,
  SkipTime,
  Source,
  TorrentSearchResult,
  TorrentTask,
  WatchHistory,
} from "./types";

async function webRequest<T>(path: string, init?: RequestInit): Promise<T> {
  const method = init?.method ?? "GET";
  const controller = new AbortController();
  const abortFromCaller = () => controller.abort(init?.signal?.reason);
  init?.signal?.addEventListener("abort", abortFromCaller, { once: true });
  const timeout = globalThis.setTimeout(
    () => controller.abort(new DOMException("any-watch request timed out", "TimeoutError")),
    90_000,
  );
  try {
    const response = await fetch(`/api${path}`, {
      ...init,
      method,
      signal: controller.signal,
      credentials: "same-origin",
      headers: {
        ...(method !== "GET" && method !== "HEAD" ? { "Content-Type": "application/json", "X-Any-Watch-Request": "1" } : {}),
        ...init?.headers,
      },
    });
    const contentType = response.headers.get("content-type") ?? "";
    const isJson = contentType.includes("application/json");
    if (!response.ok) {
      const body = isJson ? await response.json().catch(() => null) : null;
      throw body ?? {
        code: "SERVICE_UNAVAILABLE",
        message: "The any-watch service is not available at this address.",
        operation: "web-request",
        retryable: response.status >= 500,
        correlationId: crypto.randomUUID(),
      };
    }
    if (response.status === 204) return undefined as T;
    if (!isJson) {
      throw {
        code: "INVALID_SERVICE_RESPONSE",
        message: "The any-watch service returned an unexpected response.",
        operation: "web-request",
        retryable: true,
        correlationId: crypto.randomUUID(),
      };
    }
    return response.json() as Promise<T>;
  } finally {
    globalThis.clearTimeout(timeout);
    init?.signal?.removeEventListener("abort", abortFromCaller);
  }
}

const webPost = <T>(path: string, body?: unknown) =>
  webRequest<T>(path, { method: "POST", body: body === undefined ? undefined : JSON.stringify(body) });

export const api = {
  getSession: async (): Promise<SessionUser | null> => {
    try {
      const res = await webRequest<any>("/session");
      if (!res) return null;
      if (typeof res === "object" && res.user !== undefined) return res.user ?? null;
      return res as SessionUser;
    } catch (error) {
      const code = (error as { code?: string })?.code;
      if (code === "AUTH_REQUIRED" || code === "SESSION_EXPIRED") return null;
      throw error;
    }
  },
  login: (username: string, password: string) => webPost<SessionUser>("/login", { username, password }),
  logout: () => webPost<void>("/logout"),
  listUsers: () => webRequest<ManagedUser[]>("/admin/users"),
  createUser: (input: { username: string; password: string; role: string }) =>
    webPost<ManagedUser>("/admin/users", input),
  updateUser: (id: string, input: { username: string; enabled: boolean; role: string; password?: string }) =>
    webRequest<void>(`/admin/users/${encodeURIComponent(id)}`, { method: "PUT", body: JSON.stringify(input) }),
  deleteUser: (id: string) => webRequest<void>(`/admin/users/${encodeURIComponent(id)}`, { method: "DELETE" }),
  listSources: () => webRequest<Source[]>("/sources"),
  listProviderHealth: () => webRequest<Source[]>("/providers/health"),
  retryProviderHealth: (provider?: string) => webPost<Source[]>("/providers/health", { provider }),
  getDiscovery: () => webRequest<DiscoveryCatalog>("/discovery"),
  getGenreCatalog: (genre: string) => webRequest<CatalogAnime[]>(`/catalog/genre/${encodeURIComponent(genre)}`),
  getCatalog: (filters: CatalogFilters, sort: string, page = 1) => webPost<CatalogPage>("/catalog", { filters, sort, page }),
  searchCatalog: (query: string) => webRequest<CatalogAnime[]>(`/catalog/search?query=${encodeURIComponent(query)}`),
  resolveAvailability: (catalogId: number, title: string, titleVariants: string[], languageGroupFilter?: string) =>
    webPost<ProviderAvailability[]>("/availability", { catalogId, title, titleVariants, languageGroupFilter }),
  getContinueWatching: (limit = 20) => webRequest<WatchHistory[]>(`/history?limit=${limit}`),
  getMyList: (limit = 100) => webRequest<Favorite[]>(`/my-list?limit=${limit}`),
  getYouTubeTrending: (topic?: string) => webRequest<Anime[]>(
    `/youtube/trending${topic ? `?topic=${encodeURIComponent(topic)}` : ""}`,
  ),
  getYouTubePopular: () => webRequest<Anime[]>("/youtube/popular"),
  getYouTubeRelated: (videoId: string) => webRequest<Anime[]>(`/youtube/related/${encodeURIComponent(videoId)}`),
  searchSource: (source: string, query: string) => webPost<Anime[]>("/source/search", { source, query }),
  getProviderCatalog: (provider: string) => webPost<Anime[]>("/provider/catalog", { provider }),
  getAnimeDetails: (provider: string, animeId: string, title: string) =>
    webPost<AnimeDetails>("/anime/details", { provider, animeId, title }),
  getEpisodes: (provider: string, animeId: string) => webPost<Episode[]>("/anime/episodes", { provider, animeId }),
  preparePlayback: (provider: string, episodeId: string) => webPost<Playback>("/playback", { provider, episodeId }),
  getSkipTimes: (catalogId: number, episodeNumber: number) => webPost<SkipTime[]>("/skip-times", { catalogId, episodeNumber }),
  downloadEpisode: async (
    request: {
      id: string;
      provider: string;
      animeId: string;
      episodeId: string;
      animeTitle: string;
      coverUrl: string;
      episodeNumber: number;
      episodeTitle?: string | null;
    },
    onProgress: (event: DownloadEvent) => void,
  ): Promise<DownloadResult> => {
    onProgress({ event: "started", progress: 0, downloadedBytes: 0, fileName: null });
    const ticket = await webPost<{ id: string; url: string; fileName: string }>("/downloads/ticket", request);
    const link = document.createElement("a");
    link.href = ticket.url;
    link.download = ticket.fileName;
    link.hidden = true;
    document.body.append(link);
    link.click();
    link.remove();
    onProgress({ event: "finished", progress: 100, downloadedBytes: 0, fileName: ticket.fileName });
    return { id: ticket.id, fileName: ticket.fileName };
  },
  saveProgress: (progress: {
    animeId: string;
    catalogId?: number | null;
    provider: string;
    title: string;
    coverUrl: string;
    episodeNumber: number;
    episodeTitle?: string | null;
    positionSeconds: number;
    totalSeconds: number;
  }) => webPost<void>("/history", progress),
  addToMyList: (anime: Anime) => webPost<void>("/my-list", {
    id: anime.id,
    catalogId: anime.catalogId ?? null,
    provider: anime.provider,
    title: anime.title,
    coverUrl: anime.coverUrl ?? "",
  }),
  removeFromMyList: (animeId: string) => webPost<void>("/my-list/remove", { animeId }),
  removeContinueWatching: (animeId: string) => webPost<void>("/history/remove", { animeId }),
  searchTorrents: (query: string, category?: string, source?: string, page = 1) => {
    const params = new URLSearchParams({ query, page: String(page) });
    if (category) params.set("category", category);
    if (source) params.set("source", source);
    return webRequest<TorrentSearchResult[]>(`/torrents/search?${params.toString()}`);
  },
  searchTorrentSubtitles: (query: string, lang?: string) => {
    const params = new URLSearchParams({ query });
    if (lang) params.set("lang", lang);
    return webRequest<any[]>(`/torrents/subtitles?${params.toString()}`);
  },
  createTorrentTask: (
    title: string,
    magnetUrl: string,
    torrentUrl?: string | null,
    expectedSizeBytes?: number,
    subPref?: string,
  ) => webPost<TorrentTask>("/torrents/download", {
    title,
    magnet_url: magnetUrl,
    torrent_url: torrentUrl || null,
    expected_size_bytes: expectedSizeBytes || null,
    sub_pref: subPref,
  }),
  listTorrentTasks: () => webRequest<TorrentTask[]>("/torrents/tasks"),
  getTorrentTask: (id: string) => webRequest<TorrentTask>(`/torrents/tasks/${encodeURIComponent(id)}`),
  approveTorrentTask: (id: string) => webPost<TorrentTask>(`/torrents/tasks/${encodeURIComponent(id)}/approve`),
  rejectTorrentTask: (id: string) => webPost<TorrentTask>(`/torrents/tasks/${encodeURIComponent(id)}/reject`),
  deleteTorrentTask: (id: string) => webRequest<void>(`/torrents/tasks/${encodeURIComponent(id)}`, { method: "DELETE" }),
  getTorrentDownloadUrl: (id: string) => `/api/torrents/tasks/${encodeURIComponent(id)}/file`,
  getTorrentStreamUrl: (id: string) => `/api/torrents/tasks/${encodeURIComponent(id)}/stream`,
  getTorrentSubtitleUrl: (id: string, lang: string) => `/api/torrents/tasks/${encodeURIComponent(id)}/subtitles/${encodeURIComponent(lang)}`,
};

export const animeKey = (provider: string, id: string) => `${provider}:${id}`;

export const favoriteToAnime = (favorite: Favorite): Anime => ({
  id: favorite.animeId.includes(":")
    ? favorite.animeId.split(":").slice(1).join(":")
    : favorite.animeId,
  provider: favorite.provider,
  catalogId: favorite.catalogId ?? null,
  title: favorite.title,
  coverUrl: favorite.coverUrl,
  bannerUrl: null,
  language: "Saved",
  totalEpisodes: null,
  synopsis: null,
  isFavorite: true,
});
