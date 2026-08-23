import os
import time
import socket
import shutil
import subprocess
import pytest
from playwright.sync_api import sync_playwright

NPM_BIN = shutil.which("npm") or "npm"

def _free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]

@pytest.fixture(scope="session")
def vite_server():
    # Start the Vite server on a free port so concurrent sessions and CI
    # runners never collide on a fixed port.
    port = _free_port()
    proc = subprocess.Popen(
        [NPM_BIN, "run", "dev", "--", "--port", str(port)],
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    # Wait until our process serves the port; fail fast if it exited
    # (for example when the port was already taken by another server).
    start_time = time.time()
    host = "127.0.0.1"
    server_ready = False
    while time.time() - start_time < 15:
        if proc.poll() is not None:
            break
        try:
            with socket.create_connection((host, port), timeout=1):
                server_ready = True
                break
        except OSError:
            time.sleep(0.5)

    if not server_ready:
        proc.kill()
        raise RuntimeError(f"Vite dev server failed to start on port {port}")

    yield proc, port

    proc.terminate()
    proc.wait()

@pytest.fixture(scope="function")
def mocked_page(page, vite_server):
    vite_proc, vite_port = vite_server
    # Mock the browser API before the application loads.
    page.add_init_script("""
        window.__API_CALLS__ = window.__API_CALLS__ || [];

        const getMockState = () => {
            const defaults = {
                sources: [
                    { name: "AniZone", language: "English", languageGroup: "english", status: "healthy", failureCode: null, websiteUrl: "https://anizone.to", verificationUrl: null, capabilities: { search: true, details: true, episodes: true, playback: true, subtitles: true } },
                    { name: "AllAnime", language: "English", languageGroup: "english", status: "healthy", failureCode: null, websiteUrl: null, verificationUrl: "https://api.allanime.day/api", capabilities: { search: true, details: true, episodes: true, playback: true, subtitles: true } },
                    { name: "AnimeGG", language: "English", languageGroup: "english", status: "healthy", failureCode: null, websiteUrl: "https://www.animegg.org", verificationUrl: null, capabilities: { search: true, details: true, episodes: true, playback: true, subtitles: false } },
                    { name: "KKPhim", language: "Vietnamese", languageGroup: "vietnamese", status: "healthy", failureCode: null, websiteUrl: "https://www.kkphim.com", verificationUrl: null, capabilities: { search: true, details: true, episodes: true, playback: true, subtitles: false } },
                    { name: "OPhim", language: "Vietnamese", languageGroup: "vietnamese", status: "healthy", failureCode: null, websiteUrl: "https://ophim19.cc", verificationUrl: null, capabilities: { search: true, details: true, episodes: true, playback: true, subtitles: false } },
                    { name: "Invidious", language: "YouTube", languageGroup: "youtube", status: "healthy", failureCode: null, websiteUrl: "https://invidious.example", verificationUrl: null, capabilities: { search: true, details: true, episodes: true, playback: true, subtitles: true } }
                ],
                my_list: [
                    {
                        animeId: "AllAnime:naruto",
                        catalogId: 20,
                        provider: "AllAnime",
                        title: "Naruto",
                        coverUrl: "https://example.com/naruto.jpg"
                    }
                ],
                continue_watching: [
                    {
                        animeId: "AllAnime:one-piece",
                        catalogId: 21,
                        provider: "AllAnime",
                        title: "One Piece",
                        coverUrl: "https://example.com/one-piece.jpg",
                        episodeNumber: 5,
                        episodeTitle: "Episode 5",
                        positionSeconds: 300,
                        totalSeconds: 1440,
                        updatedAt: "2026-06-13T10:00:00Z"
                    }
                ],
                search_error: null,
                catalog_search_error: null,
                provider_search_error: null,
                provider_catalog_error: null,
                provider_health_error: null,
                episode_provider: null,
                skip_times: null,
                playback_error: null,
                download_error: null,
                youtube_search_delays: {},
                youtube_feed_delays: {},
                youtube_feed_error: null,
                youtube_related_delays: {},
                youtube_related_error: null,
                youtube_playback_delays: {},
                downloads: [],
                torrent_tasks: [],
                torrent_search_error: null,
                update_available: false,
                update_error: null,
                update_install_error: null,
                episode_count: 1200
            };
            const stored = localStorage.getItem('__API_MOCK_STATE__');
            if (stored) {
                try {
                    return { ...defaults, ...JSON.parse(stored) };
                } catch(e) {}
            }
            return defaults;
        };

        const saveMockState = (state) => {
            localStorage.setItem('__API_MOCK_STATE__', JSON.stringify(state));
        };

        window.__API_MOCK_STATE__ = getMockState();

        const makeYouTubeVideo = (id, title, author = "Mock Channel", meta = "1M views · 1 day ago") => ({
            id,
            provider: "Invidious",
            title,
            coverUrl: `https://example.com/youtube-${id}.jpg`,
            bannerUrl: `https://example.com/youtube-${id}-banner.jpg`,
            language: "YouTube",
            totalEpisodes: null,
            synopsis: `${author} · ${meta}\nA mock video description for ${title}.`,
            isFavorite: false
        });

        const waitForMockDelay = async (delays, key) => {
            const delay = Number(delays?.[key] || 0);
            if (delay > 0) await new Promise((resolve) => setTimeout(resolve, delay));
        };

        const invokeMock = async function(cmd, args = {}) {
            window.__API_CALLS__.push({ cmd, args });

            const state = getMockState();

            if (cmd === "list_sources") {
                return state.sources;
            } else if (cmd === "list_provider_health" && state.provider_health_error) {
                throw state.provider_health_error;
            } else if (cmd === "list_provider_health" || cmd === "retry_provider_health") {
                return state.sources;
            } else if (cmd === "get_discovery") {
                const makeCatalog = (index) => ({
                    catalogId: 1000 + index,
                    title: index === 0 ? "One Piece" : `Catalog Anime ${index + 1}`,
                    nativeTitle: null,
                    synonyms: index === 0 ? ["Đảo Hải Tặc"] : [],
                    description: `Catalog synopsis ${index + 1}.`,
                    coverUrl: `https://example.com/catalog-${index + 1}.jpg`,
                    bannerUrl: `https://example.com/catalog-banner-${index + 1}.jpg`,
                    genres: index % 2 ? ["Action"] : ["Adventure"],
                    totalEpisodes: index === 0 ? 1200 : 12,
                    score: 80 + (index % 10),
                    personalMatch: 84 + (index % 10),
                    format: "TV",
                    seasonYear: 2026
                });
                return {
                    trending: Array.from({ length: 14 }, (_, index) => makeCatalog(index)),
                    popularThisSeason: Array.from({ length: 14 }, (_, index) => makeCatalog(index + 20)),
                    genres: ["Action", "Adventure", "Comedy", "Drama", "Fantasy", "Mystery"]
                };
            } else if (cmd === "get_genre_catalog") {
                return Array.from({ length: 14 }, (_, index) => ({
                    catalogId: 2000 + index,
                    title: `${args.genre} Anime ${index + 1}`,
                    nativeTitle: null,
                    synonyms: [],
                    description: `${args.genre} catalog title.`,
                    coverUrl: `https://example.com/genre-${index + 1}.jpg`,
                    bannerUrl: null,
                    genres: [args.genre],
                    totalEpisodes: 12,
                    score: 84,
                    format: "TV",
                    seasonYear: 2026
                }));
            } else if (cmd === "get_catalog") {
                const page = args.page || 1;
                return {
                    page,
                    hasNextPage: page < 2,
                    items: Array.from({ length: 24 }, (_, index) => ({
                        catalogId: page * 10000 + index,
                        title: `${args.filters.genre || "Trending"} Anime ${index + 1}`,
                        nativeTitle: null,
                        synonyms: [],
                        description: "Catalog browser synopsis.",
                        coverUrl: `https://example.com/browser-${index + 1}.jpg`,
                        bannerUrl: null,
                        genres: [args.filters.genre || "Action"],
                        totalEpisodes: 12,
                        score: 82,
                        personalMatch: 91 - (index % 10),
                        format: "TV",
                        status: "RELEASING",
                        seasonYear: 2026
                    }))
                };
            } else if (cmd === "search_catalog") {
                if (state.catalog_search_error) throw state.catalog_search_error;
                if (state.search_error) throw state.search_error;
                if ((args.query || "").toLowerCase().includes("empty")) return [];
                if ((args.query || "").toLowerCase().includes("cinema")) return [];
                const onePiece = (args.query || "").toLowerCase().includes("one piece") || (args.query || "").toLowerCase().includes("đảo hải tặc");
                return Array.from({ length: 16 }, (_, index) => ({
                    catalogId: index === 0 && onePiece ? 21 : 3000 + index,
                    title: index === 0 ? (onePiece ? "One Piece" : "Naruto Shippuden") : `Sample Anime ${index + 1}`,
                    nativeTitle: null,
                    synonyms: index === 0 ? (onePiece ? ["Đảo Hải Tặc"] : ["Naruto: Shippuden"]) : [],
                    description: index === 0 ? "A story about Naruto." : `Sample synopsis ${index + 1}.`,
                    coverUrl: `https://example.com/search-${index + 1}.jpg`,
                    bannerUrl: `https://example.com/search-banner-${index + 1}.jpg`,
                    genres: ["Action", "Adventure"],
                    totalEpisodes: index === 0 ? 1200 : 12,
                    score: 88,
                    format: "TV",
                    seasonYear: 2026
                }));
            } else if (cmd === "resolve_availability") {
                const group = args.languageGroupFilter;
                if (group === "english") {
                    return [{ provider: "AllAnime", language: "English", status: "available", failureCode: null, anime: { id: "naruto-shippuden", catalogId: args.catalogId, provider: "AllAnime", title: args.title, coverUrl: "https://example.com/search-1.jpg", bannerUrl: null, language: "English", totalEpisodes: 1200, synopsis: null, isFavorite: false } }];
                }
                return [
                    { provider: "KKPhim", language: "Vietnamese", status: "available", failureCode: null, anime: { id: "naruto-shippuden", catalogId: args.catalogId, provider: "KKPhim", title: args.title, coverUrl: "https://example.com/search-1.jpg", bannerUrl: null, language: "Vietnamese", totalEpisodes: 1200, synopsis: null, isFavorite: false } },
                    { provider: "OPhim", language: "Vietnamese", status: "unavailable", failureCode: "TITLE_NOT_AVAILABLE", anime: null },
                    { provider: "AnimeVietSub", language: "Vietnamese", status: "available", failureCode: null, anime: { id: String(args.catalogId), catalogId: args.catalogId, provider: "AnimeVietSub", title: args.title, coverUrl: "https://example.com/search-1.jpg", bannerUrl: null, language: "Vietnamese", totalEpisodes: 1200, synopsis: null, isFavorite: false } }
                ];
            } else if (cmd === "plugin:updater|check") {
                if (state.update_error) {
                    throw new Error(state.update_error);
                }
                if (!state.update_available) {
                    return null;
                }
                return {
                    rid: 101,
                    currentVersion: "1.0.1",
                    version: "1.0.2",
                    date: "2026-06-14T00:00:00Z",
                    body: "Mock v1.0.2 updater release.",
                    rawJson: {}
                };
            } else if (cmd === "plugin:updater|download_and_install") {
                if (state.update_install_error) {
                    throw new Error(state.update_install_error);
                }
                if (args.onEvent && typeof args.onEvent.onmessage === "function") {
                    args.onEvent.onmessage({ event: "Started", data: { contentLength: 1000 } });
                    args.onEvent.onmessage({ event: "Progress", data: { chunkLength: 450 } });
                    args.onEvent.onmessage({ event: "Progress", data: { chunkLength: 550 } });
                    args.onEvent.onmessage({ event: "Finished" });
                }
                state.update_installed = true;
                saveMockState(state);
                return null;
            } else if (cmd === "plugin:process|restart") {
                state.relaunched = true;
                saveMockState(state);
                return null;
            } else if (cmd === "get_continue_watching") {
                return state.continue_watching;
            } else if (cmd === "get_my_list") {
                return state.my_list;
            } else if (cmd === "list_downloads") {
                return state.downloads;
            } else if (cmd === "open_download" || cmd === "reveal_download") {
                state.last_opened_download = args.id;
                saveMockState(state);
                return null;
            } else if (cmd === "delete_download") {
                state.downloads = state.downloads.filter(item => item.id !== args.id);
                saveMockState(state);
                return null;
            } else if (cmd === "get_my_list_catalog") {
                return state.my_list.map((item, index) => ({
                    catalogId: item.catalogId || 20 + index,
                    title: item.title,
                    nativeTitle: null,
                    description: "Saved title.",
                    coverUrl: item.coverUrl,
                    bannerUrl: null,
                    genres: ["Action"],
                    totalEpisodes: 12,
                    score: 82,
                    personalMatch: 94,
                    format: "TV",
                    seasonYear: 2026
                }));
            } else if (cmd === "get_youtube_trending") {
                if (state.youtube_feed_error) throw state.youtube_feed_error;
                const topic = args.topic || "All";
                await waitForMockDelay(state.youtube_feed_delays, topic);
                return Array.from({ length: 18 }, (_, index) =>
                    makeYouTubeVideo(
                        `${topic.toLowerCase()}-${index + 1}`,
                        index === 0 && topic === "All" ? "Saved Video" : `${topic} Video ${index + 1}`,
                        `${topic} Channel ${index + 1}`,
                    )
                );
            } else if (cmd === "get_youtube_popular") {
                if (state.youtube_feed_error) throw state.youtube_feed_error;
                await waitForMockDelay(state.youtube_feed_delays, "Popular");
                return Array.from({ length: 18 }, (_, index) =>
                    makeYouTubeVideo(`popular-${index + 1}`, `Popular Video ${index + 1}`)
                );
            } else if (cmd === "get_youtube_related") {
                if (state.youtube_related_error) throw state.youtube_related_error;
                await waitForMockDelay(state.youtube_related_delays, args.videoId);
                return Array.from({ length: 8 }, (_, index) =>
                    makeYouTubeVideo(`${args.videoId}-related-${index + 1}`, `${args.videoId} Related ${index + 1}`)
                );
            } else if (cmd === "search_source") {
                if (args.source === "Invidious") {
                    const query = args.query || "";
                    await waitForMockDelay(state.youtube_search_delays, query);
                    return Array.from({ length: 12 }, (_, index) =>
                        makeYouTubeVideo(
                            `${query.toLowerCase().replace(/[^a-z0-9]+/g, "-") || "video"}-${index + 1}`,
                            index === 0 && query.toLowerCase().includes("saved") ? "Saved Video" : `${query} Video ${index + 1}`,
                            `${query} Channel ${index + 1}`,
                        )
                    );
                }
                if (state.provider_search_error) {
                    throw new Error(state.provider_search_error);
                }
                if (state.search_error) {
                    throw new Error(state.search_error);
                }
                const query = args.query || "";
                if (query.toLowerCase().includes("empty")) {
                    return [];
                }
                if (query.toLowerCase().includes("cinema")) {
                    return [
                        {
                            id: "cinema-film",
                            provider: args.source || "KKPhim",
                            title: "Cinema Film",
                            coverUrl: "https://example.com/cinema-film.jpg",
                            bannerUrl: "https://example.com/cinema-banner.jpg",
                            language: ["AniZone", "AllAnime"].includes(args.source) ? "English" : "Vietnamese",
                            totalEpisodes: 1,
                            synopsis: "A provider-only film result.",
                            isFavorite: false
                        }
                    ];
                }
                if (query.toLowerCase().includes("one piece") && ["KKPhim", "OPhim", "Niniyo"].includes(args.source)) {
                    return [{
                        id: "dao-hai-tac",
                        provider: args.source,
                        title: "Đảo Hải Tặc",
                        coverUrl: "https://example.com/one-piece.jpg",
                        bannerUrl: "https://example.com/one-piece-banner.jpg",
                        language: "Vietnamese",
                        totalEpisodes: 1173,
                        synopsis: "A pirate adventure.",
                        isFavorite: false
                    }];
                }
                const baseResults = [
                    {
                        id: "naruto-shippuden",
                        provider: args.source || "AllAnime",
                        title: "Naruto Shippuden",
                        coverUrl: "https://example.com/naruto-shippuden.jpg",
                        bannerUrl: "https://example.com/naruto-banner.jpg",
                        language: ["AniZone", "AllAnime"].includes(args.source) ? "English" : "Vietnamese",
                        totalEpisodes: 1200,
                        synopsis: "A story about Naruto.",
                        isFavorite: false
                    },
                    {
                        id: "demon-slayer",
                        provider: args.source || "AllAnime",
                        title: "Demon Slayer",
                        coverUrl: "https://example.com/demon-slayer.jpg",
                        bannerUrl: "https://example.com/demon-banner.jpg",
                        language: ["AniZone", "AllAnime"].includes(args.source) ? "English" : "Vietnamese",
                        totalEpisodes: 26,
                        synopsis: "A story about Tanjiro.",
                        isFavorite: false
                    }
                ];
                return baseResults.concat(Array.from({ length: 14 }, (_, index) => ({
                    id: `sample-${index + 1}`,
                    provider: args.source || "AllAnime",
                    title: `Sample Anime ${index + 1}`,
                    coverUrl: `https://example.com/sample-${index + 1}.jpg`,
                    bannerUrl: `https://example.com/sample-banner-${index + 1}.jpg`,
                    language: ["AniZone", "AllAnime"].includes(args.source) ? "English" : "Vietnamese",
                    totalEpisodes: 12 + index,
                    synopsis: `Sample synopsis ${index + 1}.`,
                    isFavorite: false
                })));
            } else if (cmd === "get_provider_catalog") {
                if (state.provider_catalog_error) throw state.provider_catalog_error;
                const provider = args.provider || "AllAnime";
                return Array.from({ length: 10 }, (_, index) => ({
                    id: `${provider.toLowerCase()}-catalog-${index + 1}`,
                    provider,
                    title: `${provider} Available ${index + 1}`,
                    coverUrl: `https://example.com/${provider.toLowerCase()}-catalog-${index + 1}.jpg`,
                    bannerUrl: null,
                    language: ["AniZone", "AllAnime", "AnimeGG"].includes(provider) ? "English" : "Vietnamese",
                    totalEpisodes: 12 + index,
                    synopsis: `Available directly from ${provider}.`,
                    isFavorite: false
                }));
            } else if (cmd === "get_anime_details") {
                if (args.provider === "Invidious") {
                    await waitForMockDelay(state.youtube_playback_delays, args.animeId);
                }
                return {
                    coverUrl: "https://example.com/details.jpg",
                    bannerUrl: "https://example.com/banner.jpg",
                    totalEpisodes: state.episode_count || 1200,
                    synopsis: "Detailed synopsis of the selected anime."
                };
            } else if (cmd === "get_episodes") {
                const eps = [];
                const total = state.episode_count || 1200;
                const provider = state.episode_provider || args.provider || state.sources?.find((source) => source.status === "healthy")?.name || "AniZone";
                const certified = ["AniZone", "AniDB", "KKPhim", "OPhim", "Niniyo"].includes(provider);
                for (let i = 1; i <= total; i++) {
                    eps.push({
                        id: `ep-${i}`,
                        number: i,
                        title: `Episode ${i}`,
                        aniskipEpisodeNumber: certified ? i : null,
                        thumbnail: `https://example.com/ep-${i}.jpg`
                    });
                }
                return eps;
            } else if (cmd === "prepare_playback") {
                if (state.playback_error) {
                    throw new Error(state.playback_error);
                }
                return {
                    sessionId: "session-123",
                    playbackUrl: "https://example.com/stream.m3u8",
                    streamKind: "hls",
                    subtitles: args.provider === "AniZone"
                        ? [{ language: "English", url: "data:text/vtt,WEBVTT%0A%0A00%3A00%3A00.000%20--%3E%2000%3A00%3A05.000%0AEnglish%20subtitle" }]
                        : [],
                    qualities: ["360p", "720p", "1080p"],
                    canFallbackToMpv: true
                };
            } else if (cmd === "get_skip_times") {
                return state.skip_times || [
                    { skipType: "op", startTime: 90, endTime: 150, episodeLength: 1420 },
                    { skipType: "ed", startTime: 1320, endTime: 1410, episodeLength: 1420 }
                ];
            } else if (cmd === "download_episode") {
                if (state.download_error) {
                    throw new Error(state.download_error);
                }
                const fileName = `Episode ${String(args.request.episodeNumber).padStart(2, "0")}.ts`;
                state.last_download = args.request;
                const record = {
                    id: args.request.id,
                    provider: args.request.provider,
                    animeId: args.request.animeId,
                    animeTitle: args.request.animeTitle,
                    coverUrl: args.request.coverUrl,
                    episodeId: args.request.episodeId,
                    episodeNumber: args.request.episodeNumber,
                    episodeTitle: args.request.episodeTitle || null,
                    filePath: `/Users/test/Downloads/any-watch/${args.request.animeTitle}/${fileName}`,
                    fileName,
                    bytesDownloaded: 2048,
                    mediaKind: "hls-ts",
                    completedAt: new Date().toISOString(),
                    fileExists: true
                };
                state.downloads = [record, ...state.downloads.filter(item => item.id !== record.id)];
                saveMockState(state);
                return {
                    id: args.request.id,
                    filePath: `/Users/test/Downloads/any-watch/${args.request.animeTitle}/${fileName}`,
                    fileName,
                    bytesDownloaded: 2048,
                    mediaKind: "hls-ts"
                };
            } else if (cmd === "search_torrents") {
                if (state.torrent_search_error) throw state.torrent_search_error;
                const page = Number(args.page || 1);
                if (page > 2) return [];
                const source = args.source || "Nyaa";
                return Array.from({ length: 8 }, (_, index) => ({
                    id: `${source.toLowerCase()}-${page}-${index + 1}`,
                    title: `${args.query} Release ${page}-${index + 1} [1080p] EngSub`,
                    magnet_url: `magnet:?xt=urn:btih:${String(page * 100 + index).padStart(40, "0")}&dn=release`,
                    torrent_url: null,
                    source,
                    category: args.category === "movie" ? "Movies" : "Anime",
                    size_bytes: 1024 * 1024 * (700 + index),
                    formatted_size: `${700 + index}.0 MB`,
                    seeds: 120 - index,
                    peers: 8 + index,
                    quality: "1080p",
                    upload_date: "2026-08-21",
                    has_engsub: true,
                    has_vietsub: index % 2 === 0,
                }));
            } else if (cmd === "create_torrent_task") {
                const task = {
                    id: `torrent-task-${state.torrent_tasks.length + 1}`,
                    title: args.title,
                    magnet_url: args.magnetUrl,
                    created_at: Math.floor(Date.now() / 1000),
                    completed_at: Math.floor(Date.now() / 1000),
                    sub_pref: args.subPref || "all",
                    status: {
                        type: "ready",
                        data: {
                            file_name: "prepared-film.mp4",
                            file_size: 734003200,
                            has_mp4: true,
                            subtitles: [
                                { language: "Vietnamese", language_code: "vi", format: "vtt", file_name: "prepared-film.vi.vtt" },
                                { language: "English", language_code: "en", format: "vtt", file_name: "prepared-film.en.vtt" },
                            ],
                        },
                    },
                };
                state.torrent_tasks = [task, ...state.torrent_tasks];
                saveMockState(state);
                return task;
            } else if (cmd === "approve_torrent_task") {
                const task = state.torrent_tasks.find((t) => t.id === args.id);
                if (task) {
                    task.status = {
                        type: "ready",
                        data: {
                            file_name: "prepared-film.mp4",
                            file_size: 734003200,
                            has_mp4: true,
                            subtitles: [
                                { language: "Vietnamese", language_code: "vi", format: "vtt", file_name: "prepared-film.vi.vtt" },
                                { language: "English", language_code: "en", format: "vtt", file_name: "prepared-film.en.vtt" },
                            ],
                        },
                    };
                    saveMockState(state);
                    return task;
                }
                return null;
            } else if (cmd === "list_torrent_tasks") {
                return state.torrent_tasks;
            } else if (cmd === "get_torrent_task") {
                return state.torrent_tasks.find((task) => task.id === args.id) || null;
            } else if (cmd === "reject_torrent_task") {
                const task = state.torrent_tasks.find((t) => t.id === args.id);
                if (task) {
                    task.status = {
                        type: "rejected",
                        data: {
                            reason: args.reason || "Rejected by admin",
                            requester_name: task.status.data?.requester_name || "Viewer",
                        },
                    };
                    saveMockState(state);
                    return task;
                }
                return null;
            } else if (cmd === "get_torrent_metadata") {
                return {
                    title: args.query,
                    year: 2024,
                    description: "High quality media release available for streaming.",
                    rating: 8.8,
                    coverUrl: "https://example.com/cover.jpg",
                    bannerUrl: "https://example.com/banner.jpg",
                    genres: ["Action", "Drama"],
                    mediaType: args.category || "movie",
                };
            } else if (cmd === "delete_torrent_task") {
                state.torrent_tasks = state.torrent_tasks.filter((task) => task.id !== args.id);
                saveMockState(state);
                return null;
            } else if (cmd === "save_progress") {
                if (args.progress) {
                    const progress = args.progress;
                    const idx = state.continue_watching.findIndex(x => x.animeId === progress.animeId);
                    const item = {
                        animeId: progress.animeId,
                        provider: progress.provider,
                        title: progress.title,
                        coverUrl: progress.coverUrl,
                        episodeNumber: progress.episodeNumber,
                        episodeTitle: progress.episodeTitle || null,
                        positionSeconds: progress.positionSeconds,
                        totalSeconds: progress.totalSeconds,
                        updatedAt: new Date().toISOString()
                    };
                    if (idx !== -1) {
                        state.continue_watching[idx] = item;
                    } else {
                        state.continue_watching.push(item);
                    }
                    saveMockState(state);
                }
                return null;
            } else if (cmd === "add_to_my_list") {
                if (args.anime) {
                    const anime = args.anime;
                    const key = anime.provider + ":" + anime.id;
                    if (!state.my_list.some(x => x.animeId === key)) {
                        state.my_list.push({
                            animeId: key,
                            provider: anime.provider,
                            title: anime.title,
                            coverUrl: anime.coverUrl
                        });
                        saveMockState(state);
                    }
                }
                return null;
            } else if (cmd === "remove_from_my_list") {
                const animeId = args.animeId;
                state.my_list = state.my_list.filter(x => x.animeId !== animeId);
                saveMockState(state);
                return null;
            } else if (cmd === "remove_continue_watching") {
                const animeId = args.animeId;
                state.continue_watching = state.continue_watching.filter(x => x.animeId !== animeId);
                saveMockState(state);
                return null;
            }
            return null;
        };

        const nativeFetch = window.fetch.bind(window);
        window.fetch = async (input, init = {}) => {
            const state = getMockState();
            const url = new URL(typeof input === "string" ? input : input.url, location.href);
            if (!url.pathname.startsWith("/api")) return nativeFetch(input, init);
            const method = (init.method || "GET").toUpperCase();
            const path = url.pathname.slice(4);
            const body = init.body ? JSON.parse(String(init.body)) : {};
            let cmd;
            let args = body;

            if (method === "GET" && path === "/session") {
                if (state.session_null) {
                    return Response.json({ user: null });
                }
                return Response.json({ user: { id: "viewer-1", username: "viewer", role: "admin" } });
            } else if (method === "POST" && path === "/login") {
                state.session_null = false;
                saveMockState(state);
                return Response.json({ id: "viewer-1", username: body.username, role: "admin" });
            } else if (method === "POST" && path === "/logout") {
                state.session_null = true;
                saveMockState(state);
                return new Response(null, { status: 204 });
            } else if (method === "GET" && path === "/sources") cmd = "list_sources";
            else if (method === "GET" && path === "/providers/health") cmd = "list_provider_health";
            else if (method === "POST" && path === "/providers/health") cmd = "retry_provider_health";
            else if (method === "GET" && path === "/discovery") cmd = "get_discovery";
            else if (method === "GET" && path.startsWith("/catalog/genre/")) {
                cmd = "get_genre_catalog";
                args = { genre: decodeURIComponent(path.slice("/catalog/genre/".length)) };
            } else if (method === "POST" && path === "/catalog") cmd = "get_catalog";
            else if (method === "GET" && path === "/catalog/search") {
                cmd = "search_catalog";
                args = { query: url.searchParams.get("query") || "" };
            } else if (method === "GET" && path === "/youtube/trending") {
                cmd = "get_youtube_trending";
                args = { topic: url.searchParams.get("topic") || "All" };
            } else if (method === "GET" && path === "/youtube/popular") cmd = "get_youtube_popular";
            else if (method === "GET" && path.startsWith("/youtube/related/")) {
                cmd = "get_youtube_related";
                args = { videoId: decodeURIComponent(path.slice("/youtube/related/".length)) };
            } else if (method === "POST" && path === "/availability") cmd = "resolve_availability";
            else if (method === "GET" && path === "/history") cmd = "get_continue_watching";
            else if (method === "GET" && path === "/my-list") cmd = "get_my_list";
            else if (method === "POST" && path === "/source/search") cmd = "search_source";
            else if (method === "POST" && path === "/provider/catalog") cmd = "get_provider_catalog";
            else if (method === "POST" && path === "/anime/details") cmd = "get_anime_details";
            else if (method === "POST" && path === "/anime/episodes") cmd = "get_episodes";
            else if (method === "POST" && path === "/playback") cmd = "prepare_playback";
            else if (method === "POST" && path === "/skip-times") cmd = "get_skip_times";
            else if (method === "GET" && path === "/torrents/search") {
                cmd = "search_torrents";
                args = {
                    query: url.searchParams.get("query") || "",
                    category: url.searchParams.get("category") || "all",
                    source: url.searchParams.get("source") || "",
                    page: Number(url.searchParams.get("page") || 1),
                };
            } else if (method === "POST" && path === "/torrents/download") {
                cmd = "create_torrent_task";
                args = {
                    title: body.title,
                    magnetUrl: body.magnet_url,
                    torrentUrl: body.torrent_url,
                    expectedSizeBytes: body.expected_size_bytes,
                    subPref: body.sub_pref,
                };
            } else if (method === "GET" && path === "/torrents/tasks") cmd = "list_torrent_tasks";
            else if (method === "POST" && path.startsWith("/torrents/tasks/") && path.endsWith("/approve")) {
                args = { id: decodeURIComponent(path.slice("/torrents/tasks/".length, path.length - "/approve".length)) };
                cmd = "approve_torrent_task";
            }
            else if (path.startsWith("/torrents/tasks/") && !path.includes("/file") && !path.includes("/stream") && !path.includes("/subtitles/")) {
                args = { id: decodeURIComponent(path.slice("/torrents/tasks/".length)) };
                cmd = method === "DELETE" ? "delete_torrent_task" : "get_torrent_task";
            }
            else if (method === "POST" && path === "/downloads/ticket") {
                cmd = "download_episode";
                args = { request: body };
            } else if (method === "POST" && path === "/history") {
                cmd = "save_progress";
                args = { progress: body };
            } else if (method === "POST" && path === "/my-list") {
                cmd = "add_to_my_list";
                args = { anime: body };
            } else if (method === "POST" && path === "/my-list/remove") cmd = "remove_from_my_list";
            else if (method === "POST" && path === "/history/remove") cmd = "remove_continue_watching";

            if (!cmd) return Response.json({ code: "NOT_FOUND", message: `No mock for ${method} ${path}` }, { status: 404 });
            try {
                const result = await invokeMock(cmd, args);
                if (cmd === "download_episode") {
                    return Response.json({ id: result.id, url: "data:video/mp2t,download", fileName: result.fileName });
                }
                return result == null ? new Response(null, { status: 204 }) : Response.json(result);
            } catch (error) {
                const detail = typeof error === "object" && error !== null ? error : {
                    code: "UNEXPECTED_ERROR",
                    message: String(error),
                    operation: cmd,
                    retryable: false,
                    correlationId: "mock-error",
                };
                return Response.json(detail, { status: 503 });
            }
        };
    """)
    page.goto(f"http://127.0.0.1:{vite_port}")
    page.evaluate("localStorage.clear()")
    page.reload()
    page.wait_for_selector(".app-container, #root")
    return page

@pytest.fixture(scope="function")
def mobile_mocked_page(mocked_page):
    mocked_page.set_viewport_size({"width": 390, "height": 844})
    return mocked_page
