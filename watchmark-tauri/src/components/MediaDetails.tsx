import { logger } from "../utils/logger";
import { formatImagePath } from "../utils/imageFormat";
import { Icon } from "./ui/Icon";
import React, { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { motion, AnimatePresence } from "framer-motion";
import { Play, ArrowLeft, Star, Trash2, CloudOff, Clock, Calendar, CheckCircle2, Circle, CircleDashed } from "lucide-react";
import { formatLocaleDate, formatRuntime } from "../utils/dateFormatter";
import { useUiStore } from "../store/uiStore";
import { useAsyncInvoke } from "../hooks/useAsyncInvoke";
import { toast } from "../utils/toast";
import { tmdbUnavailableReason } from "../utils/connectivity";
import { StarRating } from "./ui/StarRating";
import { useAppStore } from "../store/useAppStore";
import { cn } from "../App";
import { RefreshCw } from "lucide-react";
import { SafeImage } from "./ui/SafeImage";

export default function MediaDetails({ mediaId, initialSeasonNum, onBack, refreshTrigger }: any) {
  const { showConfirm, setProcessing } = useUiStore();
  const { isApiAuthorized, isOffline, isCinemaMode, connectivity } = useAppStore();
  const [data, setData] = useState<any>(null);
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [activeSeason, setActiveSeason] = useState<number>(1);
  const [showFullSynopsis, setShowFullSynopsis] = useState<boolean>(false);
  const isAnimatingRef = useRef(false);
  const [contextMenu, setContextMenu] = useState<{ x: number, y: number, epId: number } | null>(null);
  const asyncInvoke = useAsyncInvoke();
  const backButtonRef = useRef<HTMLButtonElement>(null);
  const [isBackDisabled, setIsBackDisabled] = useState(false);
  const processingRef = useRef<Set<number>>(new Set());
  const [completedCountDiff, setCompletedCountDiff] = useState<number>(0);
  const [nextEpisodeInfo, setNextEpisodeInfo] = useState<any>(null);

  useEffect(() => {
    const handleClick = () => setContextMenu(null);
    document.addEventListener("click", handleClick);
    return () => document.removeEventListener("click", handleClick);
  }, []);

  useEffect(() => {
    const unlistenDelete = listen("media-deleted", (event: any) => {
      if (event.payload.media_id === mediaId) {
        setProcessing(false);
        toast.success("Media removed successfully.");
        onBack();
      }
    });

    const unlistenDeleteFailed = listen("media-delete-failed", (event: any) => {
      if (event.payload.media_id === mediaId) {
        setProcessing(false);
        toast.error(`Failed to delete media: ${event.payload.error}`);
        isAnimatingRef.current = false;
      }
    });

    return () => {
      unlistenDelete.then((f) => f());
      unlistenDeleteFailed.then((f) => f());
    };
  }, [mediaId, onBack, setProcessing]);

  useEffect(() => {
    if (backButtonRef.current) {
        backButtonRef.current.focus();
    }
  }, []);

  useEffect(() => {
    asyncInvoke("get_media_details_db", { mediaId })
      .then((res: any) => {
        if (res) {
          setData(res);
          if (res.seasons && res.seasons.length > 0) {
            let selected = res.seasons.includes(1) ? 1 : res.seasons[0];
            if (initialSeasonNum !== undefined && res.seasons.includes(initialSeasonNum)) {
                selected = initialSeasonNum;
            } else if (res.episodes && res.episodes.length > 0) {
              const unwatchedEps = res.episodes.filter((ep: any) => ep.status !== "Completed");
              if (unwatchedEps.length > 0) {
                // Find the lowest season num among unwatched
                const unwatchedSeasons = unwatchedEps.map((ep: any) => ep.season_num).filter((s: number) => s !== 0);
                if (unwatchedSeasons.length > 0) {
                  selected = Math.min(...unwatchedSeasons);
                } else {
                  // If only specials are unwatched, we can pick 0 or default to 1
                  if (res.seasons.includes(1)) {
                    selected = 1;
                  } else {
                    selected = unwatchedEps[0].season_num;
                  }
                }
              }
            }
            // Fallback: If 1 doesn't exist and no unwatched, just pick the first season that isn't 0 if possible
            if (res.episodes && res.episodes.every((ep: any) => ep.status === "Completed")) {
              const nonZeroSeasons = res.seasons.filter((s: number) => s !== 0);
              selected = nonZeroSeasons.length > 0 ? nonZeroSeasons[0] : res.seasons[0];
              // Prefer Season 1 if it exists
              if (res.seasons.includes(1)) {
                selected = 1;
              }
            }

            setActiveSeason(selected);
          }
        }
      })
      .catch(console.error);

    asyncInvoke("get_next_episode_to_play", { mediaId })
      .then((res: any) => {
        setNextEpisodeInfo(res);
      })
      .catch((err) => {
        logger.error("Failed to fetch next episode to play", err);
      });
  }, [mediaId, refreshTrigger, asyncInvoke]);

  if (!data) {
    return (
      <div className="flex h-full items-center justify-center p-8">
        <div className="w-16 h-16 border-4 border-[#FF6B00] border-t-transparent rounded-full animate-spin" />
      </div>
    );
  }

  const handleBackClick = () => {
      if (isBackDisabled) return;
      setIsBackDisabled(true);
      setTimeout(() => setIsBackDisabled(false), 500);

      // Emit local event to prompt a refetch of library view if completion counts drifted
      if (completedCountDiff !== 0) {
        const { emit } = require('@tauri-apps/api/event');
        emit('UPDATE_LIBRARY_POSTER', { mediaId });
      }

      if (window.history.length <= 2) {
         window.location.hash = "#/dashboard";
      } else {
         onBack();
      }
  };

  return (
    <motion.div
      initial={{ x: "10%", opacity: 0 }}
      animate={{ x: 0, opacity: 1 }}
      exit={{ x: "10%", opacity: 0 }}
      transition={isCinemaMode ? { type: "spring", stiffness: 300, damping: 30 } : { duration: 0 }}
      className="relative min-h-screen pb-32"
    >
      {/* Edge-to-edge Hero Banner */}
      <div className="absolute top-0 left-0 w-full h-[400px] z-0">
        <AnimatePresence mode="wait">
          <motion.div
             key={data.backdrop_path || 'fallback'}
             initial={{ opacity: 0 }}
             animate={{ opacity: 1 }}
             exit={{ opacity: 0 }}
             transition={isCinemaMode ? { duration: 0.4 } : { duration: 0 }}
             className="absolute inset-0"
          >
            <SafeImage
              srcPath={data.backdrop_path ? formatImagePath(data.backdrop_path, "w1280") : ""}
              fallbackSrcPath={data.backdrop_fallback ? formatImagePath(data.backdrop_fallback, "w1280") : undefined}
              type="backdrop"
              altText="Backdrop"
              className="w-full h-full object-cover"
            />
            {/* Body-Merge gradient overlay */}
            <div
               className="absolute inset-0"
               style={{
                   background: 'linear-gradient(to bottom, transparent 0%, rgba(13, 15, 20, 0.6) 70%, #0D0F14 100%)'
               }}
            />
          </motion.div>
        </AnimatePresence>

      </div>

      {/* Content Area */}
      <div className="relative z-10 px-12 pt-[280px] flex gap-12 items-end">
        {/* Back Button */}
        <motion.button
          ref={backButtonRef}
          onClick={handleBackClick}
          disabled={isBackDisabled}
          tabIndex={0}
          aria-label="Return to Library"
          whileHover={{
              backgroundColor: "rgba(0, 0, 0, 0.6)"
          }}
          className="flex-shrink-0 self-start flex items-center justify-center p-3 bg-black/40 backdrop-blur-md rounded-full text-white transition-all shadow-lg outline-none focus:ring-2 focus:ring-[#FF6B00] focus:ring-offset-2 focus:ring-offset-[#0D0F14] group z-50 mt-8 -ml-4"
        >
          <div className="group-hover:scale-[1.2] transition-transform duration-200">
            <ArrowLeft className="w-6 h-6 text-white" />
          </div>
        </motion.button>

        {/* Poster (overlapping banner) */}
        <motion.div
          initial={{ y: 50, opacity: 0 }}
          animate={{ y: 0, opacity: 1 }}
          transition={isCinemaMode ? { delay: 0.2 } : { duration: 0 }}
          style={{ filter: "drop-shadow(0 20px 30px rgba(0,0,0,0.9))" }}
          className="relative w-64 shrink-0 rounded-2xl overflow-hidden border border-white/10 aspect-[2/3]"
        >
          <SafeImage
            srcPath={data.poster_path ? formatImagePath(data.poster_path, "w500") : ""}
            type="poster"
            altText="Poster"
            title={data.title}
            className={`w-full h-full object-cover ${data.is_unaired ? 'grayscale-[0.5] opacity-70' : ''}`}
          />
          {data.is_unaired && (
            <div className="absolute top-2 left-2 z-20 px-2 py-1 bg-blue-500/80 backdrop-blur-md rounded-md text-[10px] font-bold text-white shadow-md uppercase tracking-wider">
              Planned
            </div>
          )}
        </motion.div>

        {/* Title and Info */}
        <motion.div
          initial={{ y: 30, opacity: 0 }}
          animate={{ y: 0, opacity: 1 }}
          transition={isCinemaMode ? { delay: 0.3 } : { duration: 0 }}
          className="flex-1 max-w-5xl"
        >

          <div className="flex items-center gap-4 mb-4">
            <h1 className="text-6xl font-extrabold tracking-tight text-white drop-shadow-lg">
              {data.title}
            </h1>
            {(() => {
              const watched = (data.episodes?.filter((e: any) => e.status === "Completed").length || 0) + completedCountDiff;
              const total = data.episodes?.filter((e: any) => e.season_num > 0)?.length || 0; // Exclude specials from total math

              if (data.type === "TV" && total > 0) {
                if (data.is_archived || watched >= total) {
                  return (
                    <motion.div
                      key="completed"
                      initial={{ scale: 0.8, opacity: 0 }}
                      animate={{ scale: [1, 1.1, 1], opacity: 1 }}
                      transition={isCinemaMode ? { duration: 0.4 } : { duration: 0 }}
                      className="px-3 py-1 bg-emerald-500/20 text-emerald-500 rounded-xl text-[11px] font-black uppercase tracking-tighter self-center whitespace-nowrap"
                    >
                      {watched} / {total} Eps • Completed
                    </motion.div>
                  );
                } else if (watched > 0) {
                  return (
                    <motion.div
                      key="watching"
                      initial={{ opacity: 0 }}
                      animate={{ opacity: 1 }}
                      className="px-3 py-1 bg-[#FF6B00]/20 text-[#FF6B00] rounded-xl text-[11px] font-black uppercase tracking-tighter self-center whitespace-nowrap"
                    >
                      {watched} / {total} Eps • Watching
                    </motion.div>
                  );
                } else {
                  return (
                    <div className="px-3 py-1 bg-white/10 text-gray-300 rounded-xl text-[11px] font-black uppercase tracking-tighter self-center whitespace-nowrap">
                      {watched} / {total} Eps • Unwatched
                    </div>
                  );
                }
              }
              return null;
            })()}
          </div>


          <div className="flex items-center gap-4 mb-3 text-sm font-bold tracking-wider flex-wrap">
            <span className={`px-3 py-1 rounded-md backdrop-blur-md text-white ${data.type === 'Unknown' ? 'bg-red-500/80' : 'bg-white/10'}`}>
              {data.type}
            </span>
            <span className="text-gray-300">Aired: {data.is_date_known ? (data.is_exact_date ? formatLocaleDate(data.release_date) : data.release_date.substring(0, 4)) : <span className="px-1.5 py-0.5 bg-gray-800 rounded text-xs font-semibold uppercase tracking-wider text-muted">TBD</span>}</span>

          <div className="flex flex-row gap-4 mb-6">
            <div className="flex flex-col items-center bg-[#1F222A] border border-[#2A2D35] rounded-xl px-4 py-3 min-w-[120px]">
              <span className="text-[#8E929C] text-[10px] font-bold tracking-widest uppercase mb-1">GLOBAL RATING</span>
              {data.vote_average === null || data.vote_average === undefined || data.vote_average === 0 ? (
                <span className="font-bold text-muted text-xl tabular-nums">NR</span>
              ) : (
                <div className="flex items-center gap-1.5 text-xl font-bold tabular-nums text-white">
                  <Icon icon={Star} className="w-5 h-5 text-[#F5C518] fill-[#F5C518]" />
                  {Number(data.vote_average).toFixed(1)}
                </div>
              )}
            </div>

            <div className="flex flex-col items-center bg-[#1F222A] border border-[#2A2D35] rounded-xl px-4 py-3 min-w-[140px]">
              <span className="text-[#8E929C] text-[10px] font-bold tracking-widest uppercase mb-1">MY RATING</span>
              <div className="mt-0.5">
                <InteractiveStarRating initialRating={data.user_rating} mediaId={data.id} />
              </div>
            </div>
          </div>


            {data.networks && data.networks.trim() !== "" && (
              <div className="flex items-center text-muted bg-white/5 px-3 py-1 rounded-md text-xs font-bold border border-white/10 uppercase">
                {data.networks}
              </div>
            )}
          </div>

          {data.genres && data.genres.trim() !== "" && (
            <div className="flex gap-2 mb-6 flex-wrap">
              {data.genres.split(",").map((g: string, i: number) => (
                <span key={i} className="px-3 py-1 rounded-full border border-white/20 bg-[#1F222A] text-gray-300 text-xs font-bold shadow-md tracking-wider">
                  {g.trim()}
                </span>
              ))}
            </div>
          )}

          <div className="mb-8">
            <p className="text-lg text-gray-300 leading-relaxed drop-shadow-md whitespace-pre-line">
              {!data.synopsis || data.synopsis.trim() === ""
                ? "No overview available."
                : (showFullSynopsis || data.synopsis.length <= 300)
                ? data.synopsis
                : `${data.synopsis.substring(0, 300)}...`}
            </p>
            {data.synopsis && data.synopsis.length > 300 && (
              <button
                onClick={() => setShowFullSynopsis(!showFullSynopsis)}
                className="mt-2 text-[#FF6B00] hover:text-[#FF8533] font-bold text-sm"
              >
                {showFullSynopsis ? "View Less" : "View More"}
              </button>
            )}
          </div>

          <div className="flex gap-4">
            <button
              onClick={async () => {
                if (!isApiAuthorized) {
                    toast.error("API Key required to refresh data.");
                    return;
                }
                if (isOffline) {
                    toast.error(tmdbUnavailableReason(connectivity));
                    return;
                }
                logger.click(`'Refresh Data' for '${data.title}'`);
                setIsRefreshing(true);
                try {
                  await asyncInvoke("add_to_tracker", {
                    tmdbId: data.tmdb_id,
                    mediaType: data.type,
                    archive: false
                  });
                  const res: any = await asyncInvoke("get_media_details_db", { mediaId });
                  if (res) {
                      setData(res);
                      logger.ipcSuccess(`Metadata Refreshed from TMDB for '${data.title}'.`);
                  }
                  toast.success("Data Refreshed Successfully");
                } catch (e: any) {
                  logger.error("Metadata Refresh Failed", e);
                  toast.error(`Refresh Failed: ${e}`);
                } finally {
                  setIsRefreshing(false);
                }
              }}
              disabled={isRefreshing || !isApiAuthorized || isOffline}
              title={isOffline ? tmdbUnavailableReason(connectivity) : !isApiAuthorized ? "API Key required" : "Refresh metadata from TMDB"}
              className={cn(
                "flex items-center gap-2 px-6 py-4 font-bold rounded-full backdrop-blur-md transition-all",
                isRefreshing || !isApiAuthorized || isOffline
                  ? "bg-white/5 text-gray-500 cursor-not-allowed opacity-50 grayscale pointer-events-none"
                  : "bg-white/10 hover:bg-white/20 text-white hover:scale-105"
              )}
            >
              <RefreshCw className={cn("w-5 h-5", isRefreshing && "animate-spin")} /> Refresh Data
            </button>
            {data.is_unaired ? (
              <div className="flex items-center gap-2 px-8 py-4 bg-gray-700/50 text-gray-300 font-bold rounded-full backdrop-blur-md cursor-not-allowed">
                <Clock className="w-5 h-5" /> 📅 Coming Soon
              </div>
            ) : (
              <button
                onClick={() => {
                  if (!nextEpisodeInfo) {
                    toast.info("No local video file linked to play. Click the file link icon on any episode below to attach a video file.", {
                      duration: 5000,
                    });
                    return;
                  }
                  logger.click(`'Play Next' -> S${nextEpisodeInfo.season_num}E${nextEpisodeInfo.ep_num}`);
                  invoke("play_episode_cmd", {
                    episodeId: nextEpisodeInfo.episode_id,
                    filePath: nextEpisodeInfo.file_path,
                    lastPosition: nextEpisodeInfo.last_position,
                  }).catch(err => {
                    toast.error(`VLC Launch Failed: ${err}`);
                  });
                }}
                title={!nextEpisodeInfo ? "No linked local files found. Click to learn how to link files." : `Play S${nextEpisodeInfo.season_num}E${nextEpisodeInfo.ep_num}`}
                className={cn(
                  "flex items-center gap-2 px-8 py-4 font-bold rounded-full transition-all shadow-lg cursor-pointer",
                  !nextEpisodeInfo
                    ? "bg-[#FF6B00]/25 hover:bg-[#FF6B00]/40 text-orange-200 border border-[#FF6B00]/30 hover:scale-105 active:scale-95"
                    : "bg-[#FF6B00] hover:bg-[#E66000] text-white shadow-[0_4px_14px_0_rgba(255,107,0,0.39)] hover:scale-105 active:scale-95"
                )}
              >
                {!nextEpisodeInfo ? (
                  <CloudOff className="w-5 h-5 text-orange-300" />
                ) : (
                  <Icon icon={Play} className="w-5 h-5" strokeWidth={2.5} fill="currentColor" />
                )}
                <div className="flex flex-col items-start">
                  <span>Play Next</span>
                  {nextEpisodeInfo?.is_gap && nextEpisodeInfo?.missing_ep_num && (
                    <span className="text-[10px] opacity-80 font-normal leading-tight">
                      Episode {nextEpisodeInfo.missing_ep_num} missing. Playing Episode {nextEpisodeInfo.ep_num}.
                    </span>
                  )}
                  {!nextEpisodeInfo && (
                    <span className="text-[10px] opacity-70 font-normal leading-tight">
                      Link file to play
                    </span>
                  )}
                </div>
              </button>
            )}
            <button
              onClick={async () => {
                logger.click(`'Mark Season Watched' for Season ${activeSeason}`);
                logger.ipcSend("mark_season_watched", `Season ${activeSeason}`);
                try {
                  await invoke("mark_season_watched", { mediaId, seasonNum: activeSeason, archiveMode: false });
                  logger.ipcSuccess(`Marked Season ${activeSeason} as Watched.`);
                  asyncInvoke("get_media_details_db", { mediaId }).then((res: any) => { if (res) setData(res); });
                  toast.success(`Marked Season ${activeSeason} as Watched`);
                } catch (e: any) {
                  logger.error("Mark Season Watched Failed", e);
                  toast.error(`Failed: ${e}`);
                }
              }}
              className="px-8 py-4 bg-white/10 hover:bg-white/20 text-white font-bold rounded-full backdrop-blur-md transition-all hover:scale-105">
              Mark Season Watched
            </button>
            <button
              onClick={async () => {
                if (isAnimatingRef.current) return;
                logger.click(`'Remove from Library' for '${data.title}'`);
                try {
                  const historyCount = await invoke<number>("get_media_history_count", { mediaId });
                  let warningMessage = "Are you sure you want to remove this show? This action cannot be undone.";
                  if (historyCount > 0) {
                    warningMessage = `This will permanently delete ${historyCount} entries from your Watch Diary. ` + warningMessage;
                  }

                  const confirmDelete = await showConfirm("Remove Media", warningMessage);
                  if (confirmDelete) {
                    isAnimatingRef.current = true;
                    setProcessing(true, `Removing ${data?.title || 'Show'} from Library...`);
                    try {
                      // We push this to the background queue, and wait for the event listener to transition back
                      await invoke("delete_media_cmd", { mediaId });
                      logger.ipcSuccess(`'${data.title}' removal task enqueued.`);
                    } catch (e: any) {
                      setProcessing(false);
                      logger.error("Failed to enqueue delete media task", e);
                      toast.error("Failed to start deletion.");
                      isAnimatingRef.current = false;
                    }
                  } else {
                      logger.action(`User cancelled 'Remove Show'`);
                  }
                } catch (e: any) {
                  logger.error("Failed to get history count", e);
                  toast.error("Failed to check media history.");
                }
              }}
              className="p-4 bg-white/5 hover:bg-red-500/20 text-muted hover:text-red-500 rounded-full transition-colors ml-auto border border-transparent hover:border-red-500/50"
            >
              <Icon icon={Trash2} className="w-5 h-5" />
            </button>
          </div>
        </motion.div>
      </div>

      {/* Episodes List (Mocked structure) */}
      <div className="mt-16 px-12">
        {/* Season Tabs */}

        {data.type === "TV" && data.seasons && (
          <div
            className="flex flex-row overflow-x-auto overflow-y-hidden whitespace-nowrap scrollbar-hide mb-8 pl-10 -ml-10"
            style={{
              maskImage: 'linear-gradient(to right, black 85%, transparent 100%)',
              WebkitMaskImage: 'linear-gradient(to right, black 85%, transparent 100%)'
            }}
            onWheel={(e) => {
              e.currentTarget.scrollLeft += e.deltaY;
            }}
          >
            {data.seasons.map((s: number) => (
              <div key={s} className="relative mr-2 group inline-block">
                {s === activeSeason && (
                  <motion.div
                    layoutId="activeSeason"
                    className="absolute inset-0 bg-[#1F222A] rounded-full"
                    initial={false}
                    transition={isCinemaMode ? { type: "spring", stiffness: 300, damping: 30, duration: 0.2 } : { duration: 0 }}
                  />
                )}
                <button
                  onClick={() => {
                    if (s === activeSeason) return;
                    setActiveSeason(s);
                  }}
                  className={`relative px-6 py-2 rounded-full font-bold transition-colors whitespace-nowrap inline-flex items-center gap-2 ${
                    s === activeSeason ? 'text-[#FFFFFF] bg-transparent' : 'text-[#8E929C] bg-transparent hover:text-white'
                  }`}
                  aria-label={s === data.new_season ? `Season ${s}, new season` : undefined}
                >
                  {s === 0 ? "Specials" : `Season ${s}`}
                  {s === data.new_season && (
                    <span className="px-1.5 py-0.5 rounded bg-[#FF6B00] text-white text-[9px] font-bold uppercase tracking-wider leading-none" title="Premiered recently">
                      New
                    </span>
                  )}
                </button>
              </div>
            ))}
          </div>
        )}


        <div className="flex flex-col w-full min-h-[60vh]">
          {data.episodes?.filter((ep: any) => ep.season_num === activeSeason).length === 0 ? (
            <div className="flex flex-col items-center justify-center py-20 text-center col-span-full">
              <h2 className="text-xl font-bold text-white mb-6">
                {activeSeason === 0 ? "No special features or pilots found." : "No episodes found for this season."}
              </h2>
              <button
                onClick={async () => {
                  if (!isApiAuthorized) {
                    toast.error("API Key required to refresh data.");
                    return;
                  }
                  if (isOffline) {
                    toast.error(tmdbUnavailableReason(connectivity));
                    return;
                  }
                  logger.click(`'Refresh Data' for '${data.title}' season ${activeSeason}`);
                  setIsRefreshing(true);
                  try {
                    await invoke("sync_season", { mediaId: data.id, seasonNum: activeSeason });
                    const res: any = await asyncInvoke("get_media_details_db", { mediaId });
                    if (res) {
                      setData(res);
                      logger.ipcSuccess(`Season ${activeSeason} data refreshed for '${data.title}'.`);
                    }
                    toast.success("Data Refreshed Successfully");
                  } catch (e: any) {
                    logger.error("Season Refresh Failed", e);
                    toast.error(`Refresh Failed: ${e}`);
                  } finally {
                    setIsRefreshing(false);
                  }
                }}
                disabled={isRefreshing || !isApiAuthorized || isOffline}
                className={cn(
                  "flex items-center gap-2 px-8 py-4 font-bold rounded-full transition-all shadow-lg",
                  isRefreshing || !isApiAuthorized || isOffline
                    ? "bg-white/5 text-gray-500 cursor-not-allowed opacity-50 grayscale pointer-events-none"
                    : "bg-[#FF6B00] hover:bg-[#FF8533] text-white hover:scale-105 shadow-orange-500/20"
                )}
              >
                <RefreshCw className={cn("w-5 h-5", isRefreshing && "animate-spin")} /> Refresh Data
              </button>
            </div>
          ) : (
            data.episodes?.filter((ep: any) => ep.season_num === activeSeason).map((ep: any) => {
             return <EpisodeRow key={ep.id} ep={ep} data={data} setContextMenu={setContextMenu} processingRef={processingRef} setCompletedCountDiff={setCompletedCountDiff} />
            })
          )}
        </div>
      </div>

      {/* Context Menu */}
      {contextMenu && (
        <div
          className="fixed bg-[#2A2D35] border border-white/10 shadow-2xl rounded-lg py-2 z-50 min-w-[160px]"
          style={{ top: contextMenu.y, left: contextMenu.x }}
        >
          <button
            className="w-full text-left px-4 py-2 hover:bg-white/10 text-red-400 text-sm flex items-center gap-2"
            onClick={async () => {
              logger.click(`'Unlink Local File' context menu for Episode ${contextMenu.epId}`);
              try {
                await invoke("remove_local_link", { episodeId: contextMenu.epId });
                logger.ipcSuccess(`Local link removed from Episode ${contextMenu.epId}.`);
                toast.success("Local link removed.");
                asyncInvoke("get_media_details_db", { mediaId }).then((res: any) => { if (res) setData(res); });
              } catch (e: any) {
                logger.error("Remove Link Failed", e);
                toast.error(`Failed to remove link: ${e}`);
              }
            }}
          >
            <Icon icon={CloudOff} className="w-4 h-4" /> Unlink Local File
          </button>
        </div>
      )}
    </motion.div>
  );
}

const InteractiveStarRating = React.memo(({ initialRating, mediaId }: { initialRating: number | null, mediaId: number }) => {
  const [rating, setRating] = useState(initialRating);
  const [isShaking, setIsShaking] = useState(false);
  const { isCinemaMode } = useAppStore();

  useEffect(() => {
    setRating(initialRating);
  }, [initialRating]);

  return (
    <motion.div
      animate={isShaking ? { x: [-5, 5, -5, 5, 0] } : {}}
      transition={isCinemaMode ? { duration: 0.4 } : { duration: 0 }}
    >
      <StarRating
        rating={rating}
        onChange={async (newRating) => {
          const prevRating = rating;
          setRating(newRating); // Optimistic UI
          try {
            logger.click(`'Rating' changed to ${newRating} stars`);
            logger.ipcSend("update_media_rating", `Rating ${newRating}`);
            await invoke("update_media_rating", { mediaId, rating: newRating });
          } catch (err: any) {
            logger.error("Rating Update Failed", err);
            toast.error("Failed to update rating");
            setRating(prevRating); // Roll Back
            setIsShaking(true);
            setTimeout(() => setIsShaking(false), 500);
          }
        }}
      />
    </motion.div>
  );
});

function EpisodeRow({ ep, data, setContextMenu, processingRef, setCompletedCountDiff }: any) {
  const stillUrl = ep.still_path ? formatImagePath(ep.still_path, "w500") : "";
  const fallbackUrl = data.backdrop_path ? formatImagePath(data.backdrop_path, "w1280") : "";
  const [localStatus, setLocalStatus] = useState(ep.status);
  const [localLastPosition, setLocalLastPosition] = useState(ep.last_position);
  const { isCinemaMode } = useAppStore();

  // Sync if prop updates from a full refresh
  useEffect(() => {
    setLocalStatus(ep.status);
    setLocalLastPosition(ep.last_position);
  }, [ep.status, ep.last_position]);

  return (
               <motion.div
                 key={ep.id}
                 whileHover={isCinemaMode ? { scale: 1.02, backgroundColor: "#252830" } : { backgroundColor: "#252830" }}
                 transition={isCinemaMode ? { type: "spring", stiffness: 300, damping: 20 } : { duration: 0 }}
                 className="flex items-center bg-[#1F222A] p-4 rounded-xl border border-[#2A2D35] group relative focus-within:ring-2 focus-within:ring-[#FF6B00] focus-within:ring-offset-2 focus-within:ring-offset-[#0D0F14] mb-4 min-h-[100px] max-h-[120px] overflow-hidden"
                 tabIndex={0}
                 onContextMenu={(e) => {
                   e.preventDefault();
                   if (ep.file_path) {
                     setContextMenu({ x: e.pageX, y: e.pageY, epId: ep.id });
                   }
                 }}
               >
                 <div className={`w-40 aspect-video bg-black/40 rounded-lg overflow-hidden shrink-0 relative mr-6 ${ep.is_unaired ? 'grayscale-[0.5] opacity-70' : ''}`}>
                   <SafeImage
                      srcPath={stillUrl}
                      fallbackSrcPath={fallbackUrl}
                      type="still"
                    episodeNumber={ep.ep_num}
                    altText={ep.title}
                    title={ep.title}
                    isFallbackImage={ep.is_fallback_image}
                    potentialSpoiler={ep.potential_spoiler}
                    isCompleted={ep.status === "Completed"}
                    className="w-full h-full object-cover"
                 />
                 <div className="absolute inset-0 flex items-center justify-center opacity-0 group-hover:opacity-100 bg-black/40 transition-opacity">
                    {ep.is_unaired ? (
                      <div className="w-12 h-12 rounded-full bg-gray-700/80 flex items-center justify-center text-white shadow-lg cursor-not-allowed group/tooltip relative">
                        <Calendar className="w-5 h-5" />
                        <div className="absolute -top-10 scale-0 group-hover/tooltip:scale-100 transition-transform bg-black text-white text-xs px-3 py-1 rounded-md whitespace-nowrap">
                          {ep.is_date_known ? `Airing ${formatLocaleDate(ep.air_date)}` : 'Unaired / TBD'}
                        </div>
                      </div>
                    ) : (
                      ep.file_path ? (
                        <motion.button
                          whileHover={isCinemaMode ? { scale: 1.1 } : {}}
                          transition={isCinemaMode ? { type: "spring", stiffness: 400, damping: 10 } : { duration: 0 }}
                          onClick={async (e) => {
                            e.stopPropagation();
                            logger.click(`'Play' on Episode S${ep.season_num}E${ep.ep_num}`);
                            try {
                                const validation: any = await invoke("validate_and_hash_file", { episodeId: ep.id, filePath: ep.file_path });
                                if (validation.status === "missing" || validation.status === "corrupted") {
                                    logger.error("Playback Failed", `File is ${validation.status}`);
                                    toast.error(`File is ${validation.status}.`, {
                                        action: {
                                            label: "Locate",
                                            onClick: async () => {
                                                try {
                                                    const { open } = await import('@tauri-apps/plugin-dialog');
                                                    const selected = await open({
                                                        multiple: false,
                                                        title: "Locate File",
                                                    });
                                                    if (selected && typeof selected === 'string') {
                                                        await invoke("update_local_file", { episodeId: ep.id, newPath: selected });
                                                        toast.success("File linked successfully!");
                                                        // Play new file
                                                        invoke("play_episode_cmd", {
                                                            episodeId: ep.id,
                                                            filePath: selected,
                                                            lastPosition: ep.last_position,
                                                        });
                                                    }
                                                } catch (err) {
                                                    toast.error("Failed to locate file.");
                                                }
                                            }
                                        },
                                        duration: 5000,
                                    });
                                    return;
                                }
                            } catch (e) {
                                toast.error(`Validation error: ${e}`);
                                return;
                            }

                            invoke("play_episode_cmd", {
                              episodeId: ep.id,
                              filePath: ep.file_path,
                              lastPosition: ep.last_position,
                            }).catch(e => {
                                if (e === "VLC_AUTH_ERROR") {
                                  toast.error("VLC Authentication Error: Failed to inject dynamic password or bind to port.", { duration: 8000 });
                                } else {
                                  toast.error(`VLC Launch Failed: ${e}`);
                                }
                            });
                          }}
                          className="w-12 h-12 flex items-center justify-center rounded-full bg-[#FF6B00] text-white shadow-[0_0_10px_rgba(255,107,0,0.3)] transition-shadow"
                        >
                          <Icon icon={Play} className="w-5 h-5 ml-1" fill="currentColor" />
                        </motion.button>
                      ) : (
                        <motion.button
                          whileHover={isCinemaMode ? { scale: 1.1 } : {}}
                          transition={isCinemaMode ? { type: "spring", stiffness: 400, damping: 10 } : { duration: 0 }}
                          onClick={async (e) => {
                            e.stopPropagation();
                            logger.click(`'Locate & Link' on Episode S${ep.season_num}E${ep.ep_num}`);
                            try {
                              const { open } = await import('@tauri-apps/plugin-dialog');
                              const selected = await open({
                                multiple: false,
                                title: `Locate File for S${ep.season_num}E${ep.ep_num}`,
                                filters: [{ name: 'Video Files', extensions: ['mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'm4v'] }],
                              });
                              if (selected && typeof selected === 'string') {
                                await invoke("update_local_file", { episodeId: ep.id, newPath: selected });
                                toast.success("File linked successfully!");
                                invoke("play_episode_cmd", {
                                  episodeId: ep.id,
                                  filePath: selected,
                                  lastPosition: ep.last_position || 0,
                                }).catch(err => toast.error(`VLC Launch Failed: ${err}`));
                              }
                            } catch (err) {
                              toast.error("Failed to link file.");
                            }
                          }}
                          className="group/tooltip relative w-12 h-12 flex items-center justify-center rounded-full bg-black/60 hover:bg-[#FF6B00] text-gray-300 hover:text-white transition-all shadow-lg cursor-pointer"
                          title="Click to link video file and play"
                        >
                          <Icon icon={CloudOff} className="w-5 h-5" />
                          <div className="absolute -top-10 scale-0 group-hover/tooltip:scale-100 transition-transform bg-black/90 backdrop-blur-md text-white text-xs px-3 py-1 rounded-md whitespace-nowrap z-30">
                            Click to link video file & play
                          </div>
                        </motion.button>
                      )
                    )}
                 </div>
                 {ep.status === "Watching" && ep.runtime > 0 && (
                    <div className="absolute bottom-0 left-0 w-full h-1 bg-white/10 flex">
                        {(() => {
                          const progress = Math.min(100, (ep.last_position / (ep.runtime * 60)) * 100);
                          if (progress <= 0) return null;
                          return (
                            <div
                                className="h-full"
                                style={{
                                  width: `${progress}%`,
                                  minWidth: "2px",
                                  backgroundColor: progress >= 90 ? "#1b5e20" : "#FF6B00"
                                }}
                            />
                          );
                        })()}
                    </div>
                 )}
               </div>

                 <div className="flex-1 overflow-hidden flex flex-col justify-center">
                 <div className="flex items-center gap-4 mb-1">
                    <button
                      onClick={async (e) => {
                        e.stopPropagation();
                        if (processingRef.current.has(ep.id)) return;
                        processingRef.current.add(ep.id);

                        logger.click(`'Toggle Status' on Episode S${ep.season_num}E${ep.ep_num}`);

                        // Optimistic UI Update: Local state only
                        const isCompleting = localStatus !== 'Completed';
                        setLocalStatus(isCompleting ? 'Completed' : 'Unwatched');
                        if (!isCompleting) setLocalLastPosition(0);
                        setCompletedCountDiff((prev: number) => prev + (isCompleting ? 1 : -1));

                        logger.ipcSend("toggle_episode_status", `Episode ${ep.id}`);
                        try {
                          await invoke("toggle_episode_status", { episodeId: ep.id });
                          logger.ipcSuccess(`Episode status toggled.`);
                        } catch (err: any) {
                          logger.error("Episode Toggle Failed", err);
                          toast.error(`Failed to update status: ${err}`);
                          // Revert on error
                          setLocalStatus(ep.status);
                          setLocalLastPosition(ep.last_position);
                          setCompletedCountDiff((prev: number) => prev - (isCompleting ? 1 : -1));
                        } finally {
                          processingRef.current.delete(ep.id);
                        }
                      }}
                      className={`w-8 h-8 shrink-0 flex items-center justify-center transition-colors`}
                    >
                      {localStatus === 'Completed' || (localLastPosition > 0 && ep.runtime > 0 && (localLastPosition / (ep.runtime * 60)) > 0.90) ? (
                        <CheckCircle2 className="w-6 h-6 text-[#10B981] fill-[#10B981]/20" strokeWidth={1.5} />
                      ) : localLastPosition > 0 && ep.runtime > 0 && (localLastPosition / (ep.runtime * 60)) >= 0.05 && (localLastPosition / (ep.runtime * 60)) <= 0.90 ? (
                        <CircleDashed
                          className="w-6 h-6 text-[#FF6B00] hover:text-[#FF6B00]/80 transition-colors"
                          strokeWidth={1.5}
                          onClick={async (e) => {
                            e.stopPropagation();
                            if (ep.file_path) {
                              logger.click(`'Resume from history' on Episode S${ep.season_num}E${ep.ep_num}`);
                              invoke("play_episode_cmd", {
                                episodeId: ep.id,
                                filePath: ep.file_path,
                                lastPosition: localLastPosition,
                              }).catch(err => {
                                toast.error(`VLC Launch Failed: ${err}`);
                              });
                            } else {
                              try {
                                const { open } = await import('@tauri-apps/plugin-dialog');
                                const selected = await open({
                                  multiple: false,
                                  title: `Locate File for S${ep.season_num}E${ep.ep_num}`,
                                  filters: [{ name: 'Video Files', extensions: ['mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'm4v'] }],
                                });
                                if (selected && typeof selected === 'string') {
                                  await invoke("update_local_file", { episodeId: ep.id, newPath: selected });
                                  toast.success("File linked successfully!");
                                  invoke("play_episode_cmd", {
                                    episodeId: ep.id,
                                    filePath: selected,
                                    lastPosition: localLastPosition || 0,
                                  }).catch(err => toast.error(`VLC Launch Failed: ${err}`));
                                }
                              } catch (err) {
                                toast.error("Failed to link file.");
                              }
                            }
                          }}
                        />
                      ) : (
                        <Circle className="w-6 h-6 text-white/20 hover:text-white/60 transition-colors" strokeWidth={1.5} />
                      )}
                    </button>
                    <div className="flex-1 flex items-center min-w-0 truncate">
                      <span className="text-[#8E929C] font-black tabular-nums mr-2">
                        {ep.ep_num}.
                      </span>
                      <h3 className={`text-xl font-bold truncate transition-colors ${localStatus === 'Completed' ? 'text-muted font-normal' : 'text-white group-hover:text-[#FF6B00]'}`}>
                        {ep.title || `Episode ${ep.ep_num}`}
                      </h3>
                    </div>
                    <div className="flex items-center ml-auto shrink-0 text-sm font-medium">
                      <span className="text-[#8E929C]">
                        {ep.runtime > 0 ? formatRuntime(ep.runtime) : '--'}
                      </span>
                      {ep.air_date && (
                        <>
                          <span className="mx-2 text-white/20">•</span>
                          {new Date(ep.air_date) > new Date() ? (
                            <span className="text-[#FF6B00] font-bold">UNAIRED</span>
                          ) : (
                            <span className="text-[#8E929C]">{formatLocaleDate(ep.air_date)}</span>
                          )}
                        </>
                      )}
                    </div>
                 </div>
                 <p className="text-[#8E929C] text-sm pl-12 line-clamp-2 mt-2">
                   {ep.overview}
                 </p>
               </div>
             </motion.div>
  );
}
