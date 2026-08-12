import os
import time
import socket
import subprocess
import pytest
from playwright.sync_api import sync_playwright

@pytest.fixture(scope="session")
def vite_server():
    # Start the Vite server
    proc = subprocess.Popen(
        ["npm", "run", "dev"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True
    )

    # Wait for the port to be open
    start_time = time.time()
    port = 1420
    host = "127.0.0.1"
    server_ready = False
    while time.time() - start_time < 15:
        try:
            with socket.create_connection((host, port), timeout=1):
                server_ready = True
                break
        except OSError:
            time.sleep(0.5)

    if not server_ready:
        proc.kill()
        raise RuntimeError("Vite dev server failed to start on port 1420")

    yield proc

    proc.terminate()
    proc.wait()

@pytest.fixture(scope="function")
def mocked_page(page, vite_server):
    # Intercept and mock window.__TAURI_INTERNALS__.invoke and window.__tauri_ipc__ before page loads
    page.add_init_script("""
        window.__TAURI_INTERNALS__ = window.__TAURI_INTERNALS__ || {};
        window.__TAURI_CALLS__ = window.__TAURI_CALLS__ || [];
        window.__TAURI_CALLBACKS__ = window.__TAURI_CALLBACKS__ || {};
        window.__TAURI_CALLBACK_ID__ = window.__TAURI_CALLBACK_ID__ || 1;
        window.__TAURI_INTERNALS__.transformCallback = window.__TAURI_INTERNALS__.transformCallback || ((callback) => {
            const id = window.__TAURI_CALLBACK_ID__++;
            window.__TAURI_CALLBACKS__[id] = callback;
            return id;
        });
        window.__TAURI_INTERNALS__.unregisterCallback = window.__TAURI_INTERNALS__.unregisterCallback || ((id) => {
            delete window.__TAURI_CALLBACKS__[id];
        });

        const getMockState = () => {
            const defaults = {
                sources: [
                    { name: "AllAnime", language: "English", languageGroup: "english", status: "healthy", failureCode: null, websiteUrl: null, verificationUrl: "https://api.allanime.day/api", capabilities: { search: true, details: true, episodes: true, playback: true, subtitles: true } },
                    { name: "AnimeGG", language: "English", languageGroup: "english", status: "healthy", failureCode: null, websiteUrl: "https://www.animegg.org", verificationUrl: null, capabilities: { search: true, details: true, episodes: true, playback: true, subtitles: false } },
                    { name: "KKPhim", language: "Vietnamese", languageGroup: "vietnamese", status: "healthy", failureCode: null, websiteUrl: "https://www.kkphim.com", verificationUrl: null, capabilities: { search: true, details: true, episodes: true, playback: true, subtitles: false } },
                    { name: "OPhim", language: "Vietnamese", languageGroup: "vietnamese", status: "healthy", failureCode: null, websiteUrl: "https://ophim19.cc", verificationUrl: null, capabilities: { search: true, details: true, episodes: true, playback: true, subtitles: false } }
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
                playback_error: null,
                download_error: null,
                downloads: [],
                update_available: false,
                update_error: null,
                update_install_error: null,
                episode_count: 1200
            };
            const stored = localStorage.getItem('__TAURI_MOCK_STATE__');
            if (stored) {
                try {
                    return { ...defaults, ...JSON.parse(stored) };
                } catch(e) {}
            }
            return defaults;
        };

        const saveMockState = (state) => {
            localStorage.setItem('__TAURI_MOCK_STATE__', JSON.stringify(state));
        };

        window.__TAURI_MOCK_STATE__ = getMockState();

        window.__TAURI_INTERNALS__.invoke = async function(cmd, args) {
            console.log("Mocked Invoke called:", cmd, args);
            window.__TAURI_CALLS__.push({ cmd, args });

            const state = getMockState();

            if (cmd === "list_sources") {
                return state.sources;
            } else if (cmd === "list_provider_health" || cmd === "retry_provider_health") {
                return state.sources;
            } else if (cmd === "get_discovery") {
                const makeCatalog = (index) => ({
                    catalogId: 1000 + index,
                    title: index === 0 ? "One Piece" : `Catalog Anime ${index + 1}`,
                    nativeTitle: null,
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
                return Array.from({ length: 16 }, (_, index) => ({
                    catalogId: 3000 + index,
                    title: index === 0 ? "Naruto Shippuden" : `Sample Anime ${index + 1}`,
                    nativeTitle: null,
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
            } else if (cmd === "search_source") {
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
                            language: args.source === "AllAnime" ? "English" : "Vietnamese",
                            totalEpisodes: 1,
                            synopsis: "A provider-only film result.",
                            isFavorite: false
                        }
                    ];
                }
                const baseResults = [
                    {
                        id: "naruto-shippuden",
                        provider: args.source || "AllAnime",
                        title: "Naruto Shippuden",
                        coverUrl: "https://example.com/naruto-shippuden.jpg",
                        bannerUrl: "https://example.com/naruto-banner.jpg",
                        language: args.source === "AllAnime" ? "English" : "Vietnamese",
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
                        language: args.source === "AllAnime" ? "English" : "Vietnamese",
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
                    language: args.source === "AllAnime" ? "English" : "Vietnamese",
                    totalEpisodes: 12 + index,
                    synopsis: `Sample synopsis ${index + 1}.`,
                    isFavorite: false
                })));
            } else if (cmd === "get_anime_details") {
                return {
                    coverUrl: "https://example.com/details.jpg",
                    bannerUrl: "https://example.com/banner.jpg",
                    totalEpisodes: state.episode_count || 1200,
                    synopsis: "Detailed synopsis of the selected anime."
                };
            } else if (cmd === "get_episodes") {
                const eps = [];
                const total = state.episode_count || 1200;
                for (let i = 1; i <= total; i++) {
                    eps.push({
                        id: `ep-${i}`,
                        number: i,
                        title: `Episode ${i}`,
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
                    subtitles: [],
                    qualities: ["360p", "720p", "1080p"],
                    canFallbackToMpv: true
                };
            } else if (cmd === "get_skip_times") {
                return [{ skipType: "op", startTime: 90, endTime: 150 }];
            } else if (cmd === "download_episode") {
                if (state.download_error) {
                    throw new Error(state.download_error);
                }
                const fileName = `Episode ${String(args.request.episodeNumber).padStart(2, "0")}.ts`;
                if (args.onEvent && typeof args.onEvent.onmessage === "function") {
                    args.onEvent.onmessage({ event: "started", progress: 0, downloadedBytes: 0, totalBytes: null, completedSegments: null, totalSegments: 2, fileName });
                    args.onEvent.onmessage({ event: "progress", progress: 50, downloadedBytes: 1024, totalBytes: null, completedSegments: 1, totalSegments: 2, fileName: null });
                    args.onEvent.onmessage({ event: "progress", progress: 100, downloadedBytes: 2048, totalBytes: null, completedSegments: 2, totalSegments: 2, fileName: null });
                }
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
                    filePath: `/Users/test/Downloads/ani-desk/${args.request.animeTitle}/${fileName}`,
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
                    filePath: `/Users/test/Downloads/ani-desk/${args.request.animeTitle}/${fileName}`,
                    fileName,
                    bytesDownloaded: 2048,
                    mediaKind: "hls-ts"
                };
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

        window.__tauri_ipc__ = async function(message) {
            console.log("Mocked IPC message called:", message);
            return null;
        };
    """)
    page.goto("http://127.0.0.1:1420")
    page.evaluate("localStorage.clear()")
    page.reload()
    page.wait_for_selector(".app-container, #root")
    return page

@pytest.fixture(scope="function")
def mobile_mocked_page(mocked_page):
    mocked_page.set_viewport_size({"width": 390, "height": 844})
    return mocked_page
