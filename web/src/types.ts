export type Source = {
  name: string;
  language: string;
  languageGroup: "english" | "vietnamese" | string;
  status: "healthy" | "degraded" | "unavailable" | "unknown" | string;
  failureCode?: string | null;
  capabilities: ProviderCapabilities;
  websiteUrl?: string | null;
};

export type ProviderCapabilities = {
  search: boolean;
  details: boolean;
  episodes: boolean;
  playback: boolean;
  subtitles: boolean;
};

export type CatalogAnime = {
  catalogId: number;
  title: string;
  nativeTitle?: string | null;
  synonyms?: string[];
  description?: string | null;
  coverUrl: string;
  bannerUrl?: string | null;
  genres: string[];
  totalEpisodes?: number | null;
  score?: number | null;
  format?: string | null;
  seasonYear?: number | null;
  season?: string | null;
  status?: string | null;
  popularity?: number | null;
  trending?: number | null;
  personalMatch?: number | null;
};

export type CatalogFilters = {
  genre?: string | null;
  season?: string | null;
  year?: number | null;
  format?: string | null;
  status?: string | null;
};

export type CatalogPage = {
  items: CatalogAnime[];
  page: number;
  hasNextPage: boolean;
};

export type DiscoveryCatalog = {
  trending: CatalogAnime[];
  popularThisSeason: CatalogAnime[];
  genres: string[];
};

export type ProviderAvailability = {
  provider: string;
  language: string;
  status: "available" | "unavailable" | string;
  failureCode?: string | null;
  anime?: Anime | null;
};

export type AppError = {
  code: string;
  message: string;
  provider?: string | null;
  operation: string;
  retryable: boolean;
  correlationId: string;
  technical?: string | null;
};

export type SessionUser = {
  id: string;
  username: string;
  role: "admin" | "user" | "guest";
};

export type ManagedUser = {
  id: string;
  username: string;
  role: "admin" | "user" | "guest";
  enabled: boolean;
  protected: boolean;
  createdAt: string;
};

export type Anime = {
  id: string;
  catalogId?: number | null;
  provider: string;
  title: string;
  coverUrl: string;
  bannerUrl?: string | null;
  language: string;
  totalEpisodes?: number | null;
  synopsis?: string | null;
  isFavorite: boolean;
};

export type AnimeDetails = {
  coverUrl?: string | null;
  bannerUrl?: string | null;
  totalEpisodes?: number | null;
  synopsis?: string | null;
};

export type Episode = {
  id: string;
  number: number;
  aniskipEpisodeNumber?: number | null;
  title?: string | null;
  thumbnail?: string | null;
};

export type WatchHistory = {
  animeId: string;
  catalogId?: number | null;
  provider: string;
  title: string;
  coverUrl: string;
  episodeNumber: number;
  episodeTitle?: string | null;
  positionSeconds: number;
  totalSeconds: number;
  updatedAt: string;
};

export type Favorite = {
  animeId: string;
  catalogId?: number | null;
  provider: string;
  title: string;
  coverUrl: string;
};

export type Playback = {
  sessionId: string;
  playbackUrl: string;
  streamKind: "hls" | "native" | string;
  subtitles: Array<{ language: string; url: string }>;
  qualities: string[];
};

export type SkipTime = {
  skipType: "op" | "ed" | "recap" | string;
  startTime: number;
  endTime: number;
  episodeLength?: number | null;
};

export type DownloadEvent = {
  event: "started" | "progress" | "finished" | string;
  progress: number;
  downloadedBytes: number;
  totalBytes?: number | null;
  completedSegments?: number | null;
  totalSegments?: number | null;
  fileName?: string | null;
};

export type DownloadResult = {
  id: string;
  fileName: string;
};

export type EpisodeDownloadState = {
  status: "preparing" | "downloading" | "complete" | "error";
  progress: number;
  downloadId?: string;
  provider?: string;
  animeId?: string;
  animeTitle?: string;
  coverUrl?: string;
  episodeId?: string;
  episodeNumber?: number;
  episodeTitle?: string | null;
  message?: string;
  fileName?: string;
};

export type YouTubeVideoItem = {
  id: string;
  title: string;
  author: string;
  authorId?: string;
  authorUrl?: string;
  thumbnail: string;
  viewCount?: number;
  publishedText?: string;
  lengthSeconds?: number;
  description?: string;
};

export type YouTubeTopic = "All" | "Trending" | "Music" | "Films" | "Anime" | "Gaming" | "News";

export type PlayerContext = {
  anime: Anime;
  episode: Episode;
  episodes: Episode[];
  playback: Playback;
  startTime: number;
};

export type TorrentSearchResult = {
  id: string;
  title: string;
  magnet_url: string;
  torrent_url?: string | null;
  source: string;
  category: string;
  size_bytes: number;
  formatted_size: string;
  seeds: number;
  peers: number;
  quality?: string | null;
  upload_date?: string | null;
  has_engsub: boolean;
  has_vietsub: boolean;
};

export type TorrentSubtitleMeta = {
  language: string;
  language_code: string;
  format: string;
  file_name: string;
};

export type TorrentTaskStatus =
  | {
      type: "pending_approval";
      data: {
        requester_id: string;
        requester_name: string;
      };
    }
  | { type: "queued" }
  | {
      type: "downloading";
      data: {
        progress: number;
        speed_bytes_per_sec: number;
        eta_seconds: number;
        downloaded_bytes: number;
        total_bytes: number;
      };
    }
  | {
      type: "remuxing";
      data: {
        progress: number;
        message: string;
      };
    }
  | {
      type: "ready";
      data: {
        file_name: string;
        file_size: number;
        has_mp4: boolean;
        subtitles: TorrentSubtitleMeta[];
      };
    }
  | {
      type: "failed";
      data: {
        reason: string;
      };
    };

export type TorrentTask = {
  id: string;
  title: string;
  magnet_url: string;
  status: TorrentTaskStatus;
  created_at: number;
  completed_at?: number | null;
  sub_pref?: string | null;
};

