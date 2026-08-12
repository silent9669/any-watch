import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  Anime,
  AnimeDetails,
  CatalogAnime,
  CatalogFilters,
  CatalogPage,
  DiscoveryCatalog,
  DownloadEvent,
  DownloadRecord,
  DownloadResult,
  Episode,
  Favorite,
  ManagedUser,
  Playback,
  ProviderAvailability,
  SessionUser,
  SkipTime,
  Source,
  WatchHistory,
} from "./types";

export const isNativeRuntime = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

async function webRequest<T>(path: string, init?: RequestInit): Promise<T> {
  const method = init?.method ?? "GET";
  const controller = new AbortController();
  const abortFromCaller = () => controller.abort(init?.signal?.reason);
  init?.signal?.addEventListener("abort", abortFromCaller, { once: true });
  const timeout = globalThis.setTimeout(
    () => controller.abort(new DOMException("ani-desk request timed out", "TimeoutError")),
    90_000,
  );
  try {
    const response = await fetch(`/api${path}`, {
      ...init,
      method,
      signal: controller.signal,
      credentials: "same-origin",
      headers: {
        ...(method !== "GET" && method !== "HEAD" ? { "Content-Type": "application/json", "X-Ani-Desk-Request": "1" } : {}),
        ...init?.headers,
      },
    });
    const contentType = response.headers.get("content-type") ?? "";
    const isJson = contentType.includes("application/json");
    if (!response.ok) {
      const body = isJson ? await response.json().catch(() => null) : null;
      throw body ?? {
        code: "SERVICE_UNAVAILABLE",
        message: "The ani-desk service is not available at this address.",
        operation: "web-request",
        retryable: response.status >= 500,
        correlationId: crypto.randomUUID(),
      };
    }
    if (response.status === 204) return undefined as T;
    if (!isJson) {
      throw {
        code: "INVALID_SERVICE_RESPONSE",
        message: "The ani-desk service returned an unexpected response.",
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
    if (isNativeRuntime()) return { id: "local-device", username: "Local", role: "admin", hosted: false };
    try {
      const user = await webRequest<Omit<SessionUser, "hosted">>("/session");
      return { ...user, hosted: true };
    } catch (error) {
      const code = (error as { code?: string })?.code;
      if (code === "AUTH_REQUIRED" || code === "SESSION_EXPIRED") return null;
      throw error;
    }
  },
  login: async (username: string, password: string): Promise<SessionUser> => {
    const user = await webPost<Omit<SessionUser, "hosted">>("/login", { username, password });
    return { ...user, hosted: true };
  },
  logout: () => webPost<void>("/logout"),
  listUsers: () => webRequest<ManagedUser[]>("/admin/users"),
  createUser: (input: { username: string; password: string; role: string }) =>
    webPost<ManagedUser>("/admin/users", input),
  updateUser: (id: string, input: { username: string; enabled: boolean; role: string; password?: string }) =>
    webRequest<void>(`/admin/users/${encodeURIComponent(id)}`, { method: "PUT", body: JSON.stringify(input) }),
  deleteUser: (id: string) =>
    webRequest<void>(`/admin/users/${encodeURIComponent(id)}`, { method: "DELETE" }),

  listSources: () => isNativeRuntime() ? invoke<Source[]>("list_sources") : webRequest<Source[]>("/sources"),
  listProviderHealth: () => isNativeRuntime() ? invoke<Source[]>("list_provider_health") : webRequest<Source[]>("/providers/health"),
  retryProviderHealth: (provider?: string) => isNativeRuntime()
    ? invoke<Source[]>("retry_provider_health", { provider })
    : webPost<Source[]>("/providers/health", { provider }),
  getDiscovery: () => isNativeRuntime() ? invoke<DiscoveryCatalog>("get_discovery") : webRequest<DiscoveryCatalog>("/discovery"),
  getGenreCatalog: (genre: string) => isNativeRuntime()
    ? invoke<CatalogAnime[]>("get_genre_catalog", { genre })
    : webRequest<CatalogAnime[]>(`/catalog/genre/${encodeURIComponent(genre)}`),
  getCatalog: (filters: CatalogFilters, sort: string, page = 1) => isNativeRuntime()
    ? invoke<CatalogPage>("get_catalog", { filters, sort, page })
    : webPost<CatalogPage>("/catalog", { filters, sort, page }),
  searchCatalog: (query: string) => isNativeRuntime()
    ? invoke<CatalogAnime[]>("search_catalog", { query })
    : webRequest<CatalogAnime[]>(`/catalog/search?query=${encodeURIComponent(query)}`),
  resolveAvailability: (catalogId: number, title: string, languageGroupFilter?: string) => isNativeRuntime()
    ? invoke<ProviderAvailability[]>("resolve_availability", { catalogId, title, languageGroupFilter })
    : webPost<ProviderAvailability[]>("/availability", { catalogId, title, languageGroupFilter }),
  getContinueWatching: (limit = 20) => isNativeRuntime()
    ? invoke<WatchHistory[]>("get_continue_watching", { limit })
    : webRequest<WatchHistory[]>(`/history?limit=${limit}`),
  getMyList: (limit = 100) => isNativeRuntime()
    ? invoke<Favorite[]>("get_my_list", { limit })
    : webRequest<Favorite[]>(`/my-list?limit=${limit}`),
  searchSource: (source: string, query: string) => isNativeRuntime()
    ? invoke<Anime[]>("search_source", { source, query })
    : webPost<Anime[]>("/source/search", { source, query }),
  getAnimeDetails: (provider: string, animeId: string, title: string) => isNativeRuntime()
    ? invoke<AnimeDetails>("get_anime_details", { provider, animeId, title })
    : webPost<AnimeDetails>("/anime/details", { provider, animeId, title }),
  getEpisodes: (provider: string, animeId: string) => isNativeRuntime()
    ? invoke<Episode[]>("get_episodes", { provider, animeId })
    : webPost<Episode[]>("/anime/episodes", { provider, animeId }),
  preparePlayback: (provider: string, episodeId: string) => isNativeRuntime()
    ? invoke<Playback>("prepare_playback", { provider, episodeId })
    : webPost<Playback>("/playback", { provider, episodeId }),
  getSkipTimes: (catalogId: number, episodeNumber: number) => isNativeRuntime()
    ? invoke<SkipTime[]>("get_skip_times", { catalogId, episodeNumber })
    : webPost<SkipTime[]>("/skip-times", { catalogId, episodeNumber }),
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
  ) => {
    if (isNativeRuntime()) {
      const onEvent = new Channel<DownloadEvent>();
      onEvent.onmessage = onProgress;
      return invoke<DownloadResult>("download_episode", { request, onEvent });
    }
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
    return { id: ticket.id, filePath: "Browser Downloads", fileName: ticket.fileName, bytesDownloaded: 0, mediaKind: "browser" };
  },
  listDownloads: (limit = 500) => isNativeRuntime() ? invoke<DownloadRecord[]>("list_downloads", { limit }) : Promise.resolve([]),
  openDownload: (id: string) => isNativeRuntime() ? invoke<void>("open_download", { id }) : Promise.reject(new Error("Local download management is available in the desktop app.")),
  revealDownload: (id: string) => isNativeRuntime() ? invoke<void>("reveal_download", { id }) : Promise.reject(new Error("Local download management is available in the desktop app.")),
  deleteDownload: (id: string) => isNativeRuntime() ? invoke<void>("delete_download", { id }) : Promise.reject(new Error("Local download management is available in the desktop app.")),
  openInMpv: (provider: string, episodeId: string, startTime?: number) => isNativeRuntime()
    ? invoke<void>("open_in_mpv", { provider, episodeId, startTime: Math.max(0, Math.floor(startTime ?? 0)) })
    : Promise.reject(new Error("MPV fallback is available in the desktop app.")),
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
  }) => isNativeRuntime() ? invoke<void>("save_progress", { progress }) : webPost<void>("/history", progress),
  addToMyList: (anime: Anime) => {
    const value = {
      id: anime.id,
      catalogId: anime.catalogId ?? null,
      provider: anime.provider,
      title: anime.title,
      coverUrl: anime.coverUrl ?? "",
    };
    return isNativeRuntime() ? invoke<void>("add_to_my_list", { anime: value }) : webPost<void>("/my-list", value);
  },
  removeFromMyList: (animeId: string) => isNativeRuntime()
    ? invoke<void>("remove_from_my_list", { animeId })
    : webPost<void>("/my-list/remove", { animeId }),
  removeContinueWatching: (animeId: string) => isNativeRuntime()
    ? invoke<void>("remove_continue_watching", { animeId })
    : webPost<void>("/history/remove", { animeId }),
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
