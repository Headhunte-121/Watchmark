import { logger } from "../utils/logger";
import { Icon } from "./ui/Icon";
import { formatImagePath } from "../utils/imageFormat";
import { formatRemainingTime } from "../utils/dateFormatter";
import { useState, useEffect, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion, AnimatePresence } from "framer-motion";
import { Play, Star, Loader2, FolderOpen, Flame } from "lucide-react";
import { useAppStore } from "../store/useAppStore";
import { useAsyncInvoke } from "../hooks/useAsyncInvoke";
import { SafeImage } from "./ui/SafeImage";
import { useHorizontalScroll } from "../hooks/useHorizontalScroll";
import { ChevronLeft, ChevronRight } from "lucide-react";
import { toast } from "sonner";
import { NewSeasonAlertBanner, NewSeasonAlert } from "./NewSeasonAlertBanner";
import { UpcomingAiringRow, UpcomingAiring } from "./UpcomingAiringRow";

// Types matching the Rust backend structure
interface Episode {
  id: number;
  media_id: number;
  show_title: string;
  title: string;
  season_num: number;
  ep_num: number;
  backdrop_path: string;
  still_path: string;
  file_path: string;
  status: string;
  last_position: number;
  runtime: number; // in minutes
  media_type: string;
  is_date_known: boolean;
  progress_percentage: number;
  spotlight_tag?: string;
  spotlight_subtitle?: string;
  is_season_finale?: boolean;
}

interface Media {
  id: number;
  title: string;
  poster_path: string;
  backdrop_path: string;
  user_rating: number | null; // rating stays nullable because not everything is rated
  total_episodes: number;
  completed_eps: number;
  media_type: string;
  is_date_known: boolean;
}

interface Stats {
  hrs_watched: string;
  shows_completed: number;
  avg_rating: number;
}

interface DashboardData {
  hero_ep: Episode | null;
  hero_eps?: EpisodeExtended[];
  cw_eps: Episode[];
  recent_media: Media[];
  stats: Stats;
}

// Unsplash placeholders
const PLACEHOLDER_BACKDROP = "https://images.unsplash.com/photo-1542293787-827fb705d15a?q=80&w=2560&auto=format&fit=crop";
const PLACEHOLDER_POSTER = "https://images.unsplash.com/photo-1534447677768-be436bb09401?q=80&w=600&auto=format&fit=crop";

interface EpisodeExtended extends Episode {
    is_fallback_image?: boolean;
    potential_spoiler?: boolean;
}

import { listen } from "@tauri-apps/api/event";

export default function Dashboard({ onMediaSelect, refreshTrigger, searchQuery = "" }: { onMediaSelect: (id: number) => void, refreshTrigger: number, searchQuery?: string }) {
  const { isCinemaMode } = useAppStore();
  const [data, setData] = useState<DashboardData | null>(null);
  const [upcomingAirings, setUpcomingAirings] = useState<UpcomingAiring[]>([]);
  const [seasonAlerts, setSeasonAlerts] = useState<NewSeasonAlert[]>([]);
  const [heroIndex, setHeroIndex] = useState(0);
  const [isHeroHovered, setIsHeroHovered] = useState(false);
  const [showProgressTooltip, setShowProgressTooltip] = useState(false);
  const [isPlaybackActive, setIsPlaybackActive] = useState(false);
  const asyncInvoke = useAsyncInvoke();
  const cwScroll = useHorizontalScroll<HTMLDivElement>();
  const recentScroll = useHorizontalScroll<HTMLDivElement>();

  const heroList = useMemo(() => {
    if (data?.hero_eps && data.hero_eps.length > 0) {
      return data.hero_eps;
    }
    return data?.hero_ep ? [data.hero_ep] : [];
  }, [data?.hero_eps, data?.hero_ep]);

  const activeHero = useMemo(() => {
    if (heroList.length === 0) return null;
    const safeIdx = ((heroIndex % heroList.length) + heroList.length) % heroList.length;
    return heroList[safeIdx];
  }, [heroList, heroIndex]);

  useEffect(() => {
    if (heroList.length <= 1 || isHeroHovered || isPlaybackActive) return;

    const interval = setInterval(() => {
      setHeroIndex((prev) => (prev + 1) % heroList.length);
    }, 8000);

    return () => clearInterval(interval);
  }, [heroList.length, isHeroHovered, isPlaybackActive]);

  const handlePrevHero = (e: React.MouseEvent) => {
    e.stopPropagation();
    setHeroIndex((prev) => (prev - 1 + heroList.length) % heroList.length);
  };

  const handleNextHero = (e: React.MouseEvent) => {
    e.stopPropagation();
    setHeroIndex((prev) => (prev + 1) % heroList.length);
  };

  useEffect(() => {
    asyncInvoke<DashboardData>("get_dashboard_data")
      .then(res => {
        if (res) setData(res);
      })
      .catch((err) => {
        logger.error("Failed to load dashboard data", err);
      });

    asyncInvoke<UpcomingAiring[]>("get_upcoming_airings")
      .then(res => {
        if (res) setUpcomingAirings(res);
      })
      .catch((err) => {
        logger.error("Failed to load upcoming airings", err);
      });

    asyncInvoke<NewSeasonAlert[]>("get_new_season_alerts")
      .then(res => {
        if (res) setSeasonAlerts(res);
      })
      .catch((err) => {
        logger.error("Failed to load new season alerts", err);
      });
  }, [refreshTrigger, asyncInvoke]);

  useEffect(() => {
    const unlistenAutoUpdate = listen("shows-auto-updated", () => {
      logger.app("Shows auto-updated event received, reloading dashboard feeds");
      asyncInvoke<UpcomingAiring[]>("get_upcoming_airings").then(res => res && setUpcomingAirings(res));
      asyncInvoke<NewSeasonAlert[]>("get_new_season_alerts").then(res => res && setSeasonAlerts(res));
      asyncInvoke<DashboardData>("get_dashboard_data").then(res => res && setData(res));
    });

    return () => {
      unlistenAutoUpdate.then(fn => fn());
    };
  }, [asyncInvoke]);

  const handleDismissAlert = async (mediaId: number) => {
    try {
      await invoke("dismiss_new_season_alert", { mediaId });
      setSeasonAlerts(prev => prev.filter(a => a.media_id !== mediaId));
    } catch (e) {
      logger.error("Failed to dismiss season alert", e);
    }
  };

  useEffect(() => {
    const unlistenEnded = listen("vlc-session-ended", () => {
      setIsPlaybackActive(false);
    });
    const unlistenClosed = listen("vlc-closed", () => {
      setIsPlaybackActive(false);
    });
    const unlistenCrashed = listen("vlc-crashed", () => {
      setIsPlaybackActive(false);
    });

    return () => {
      unlistenEnded.then(fn => fn());
      unlistenClosed.then(fn => fn());
      unlistenCrashed.then(fn => fn());
    };
  }, []);

  const heroRemainingTime = useMemo(() => {
    if (!activeHero) return null;
    const totalSeconds = activeHero.runtime * 60;
    const remainingSeconds = Math.max(0, totalSeconds - (activeHero.last_position || 0));
    return formatRemainingTime(remainingSeconds);
  }, [activeHero]);

  const heroTimestampDisplay = useMemo(() => {
    if (!activeHero) return "";
    const formatClock = (seconds: number) => {
      const mins = Math.floor(seconds / 60);
      const secs = Math.floor(seconds % 60);
      return `${mins}:${secs < 10 ? '0' : ''}${secs}`;
    };
    const pos = activeHero.last_position || 0;
    const tot = (activeHero.runtime || 0) * 60;
    return `${formatClock(pos)} / ${formatClock(tot)}`;
  }, [activeHero]);

  // Must stay above the loading early-return below: hooks have to run in the
  // same order on every render (Rules of Hooks).
  const filteredAirings = useMemo(() => {
    if (!searchQuery) return upcomingAirings;
    const q = searchQuery.toLowerCase();
    return upcomingAirings.filter(a =>
      a.show_title?.toLowerCase().includes(q) ||
      a.ep_title?.toLowerCase().includes(q)
    );
  }, [upcomingAirings, searchQuery]);

  if (!data) {
    // Skeleton Loading State
    return (
      <div className="flex-1 overflow-y-auto px-10 py-6 pb-24 scrollbar-hide animate-pulse">
        {/* Hero Skeleton */}
        <div className="relative w-full h-[450px] bg-[#1F222A] rounded-2xl overflow-hidden mb-12" />

        {/* Continue Watching Skeleton */}
        <div className="h-8 w-64 bg-[#1F222A] rounded mb-6 mt-12" />
        <div className="flex gap-6 overflow-x-hidden pb-4">
          {[1, 2, 3, 4].map(i => (
            <div key={i} className="flex-none min-w-[320px] h-[220px] bg-[#1F222A] rounded-xl" />
          ))}
        </div>

        {/* Recently Added Skeleton */}
        <div className="h-8 w-64 bg-[#1F222A] rounded mb-6 mt-12" />
        <div className="flex gap-6 overflow-x-hidden pb-8">
          {[1, 2, 3, 4, 5, 6].map(i => (
            <div key={i} className="flex-none min-w-[180px] aspect-[2/3] bg-[#1F222A] rounded-xl" />
          ))}
        </div>

        {/* Stats Skeleton */}
        <div className="h-8 w-64 bg-[#1F222A] rounded mb-6 mt-12" />
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-12">
          {[1, 2, 3].map(i => (
            <div key={i} className="h-32 bg-[#1F222A] rounded-xl" />
          ))}
        </div>
      </div>
    );
  }

  // Derived progress logic
  const calculateProgress = (lastPos: number, runtimeMins: number) => {
    if (runtimeMins <= 0) return 0;
    const progress = (lastPos / (runtimeMins * 60)) * 100;
    return Math.max(0, Math.min(100, progress));
  };


  const filteredCW: EpisodeExtended[] = data.cw_eps?.filter((ep: any) =>
    !searchQuery ||
    ep.show_title?.toLowerCase().includes(searchQuery.toLowerCase())
  ) || [];

  const filteredRecent = data.recent_media?.filter((m: any) =>
    !searchQuery ||
    m.title?.toLowerCase().includes(searchQuery.toLowerCase())
  ) || [];

  return (
    <div className="flex-1 overflow-y-auto px-10 py-6 pb-24 pt-24 scrollbar-hide relative w-full h-full">
      {/* New Season Premiered Alert Banner */}
      <NewSeasonAlertBanner
        alerts={seasonAlerts}
        onDismiss={handleDismissAlert}
        onSelectMedia={onMediaSelect}
      />

      {/* A. Hero Banner (Spotlight Carousel) */}
      {activeHero ? (
        <div
          onMouseEnter={() => setIsHeroHovered(true)}
          onMouseLeave={() => setIsHeroHovered(false)}
          className="relative w-full h-[450px] min-h-[400px] lg:h-[50vh] overflow-hidden rounded-3xl group mb-10 shadow-2xl"
        >
          {/* Background Crossfade Transition */}
          <AnimatePresence mode="popLayout">
            <motion.div
              key={activeHero.id}
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              transition={{ duration: 0.5 }}
              className="absolute inset-0 w-full h-full"
            >
              <SafeImage
                srcPath={
                  (activeHero as any).still_path
                    ? formatImagePath((activeHero as any).still_path, "w1280")
                    : activeHero.backdrop_path
                    ? formatImagePath(activeHero.backdrop_path, "w1280")
                    : ""
                }
                fallbackSrcPath={(activeHero as any).backdrop_fallback ? formatImagePath((activeHero as any).backdrop_fallback, "w1280") : undefined}
                type="backdrop"
                altText="Hero Backdrop"
                isFallbackImage={(activeHero as any).is_fallback_image}
                potentialSpoiler={(activeHero as any).potential_spoiler}
                isCompleted={activeHero.status === "Completed"}
                className="w-full h-full object-cover origin-center"
              />
            </motion.div>
          </AnimatePresence>

          {/* Layered directional gradient: Bottom-left pure black fading up to top-right transparent */}
          <div className="absolute inset-0 bg-gradient-to-tr from-[#0D0F14] from-0% via-[#0D0F14]/90 via-25% to-transparent to-65% pointer-events-none" />
          {/* Secondary overlay for high-key images */}
          <div className="absolute inset-0 bg-black/25 pointer-events-none" />

          {/* Spotlight Navigation Arrows (Visible on hover when multiple items exist) */}
          {heroList.length > 1 && (
            <>
              <button
                onClick={handlePrevHero}
                className="absolute left-4 top-1/2 -translate-y-1/2 z-30 p-2.5 rounded-full bg-black/50 hover:bg-black/80 text-white backdrop-blur-md opacity-0 group-hover:opacity-100 transition-all duration-200 hover:scale-110 cursor-pointer border border-white/10 shadow-xl"
                aria-label="Previous spotlight"
              >
                <ChevronLeft className="w-6 h-6" />
              </button>
              <button
                onClick={handleNextHero}
                className="absolute right-4 top-1/2 -translate-y-1/2 z-30 p-2.5 rounded-full bg-black/50 hover:bg-black/80 text-white backdrop-blur-md opacity-0 group-hover:opacity-100 transition-all duration-200 hover:scale-110 cursor-pointer border border-white/10 shadow-xl"
                aria-label="Next spotlight"
              >
                <ChevronRight className="w-6 h-6" />
              </button>
            </>
          )}

          {/* Top-Right Carousel Indicator Pills & Counter */}
          {heroList.length > 1 && (
            <div className="absolute top-6 right-8 z-20 flex items-center gap-2 bg-black/60 backdrop-blur-md px-3.5 py-1.5 rounded-full border border-white/10 shadow-lg">
              <span className="text-[11px] font-mono font-bold text-white/70 mr-1 tabular-nums">
                {(((heroIndex % heroList.length) + heroList.length) % heroList.length) + 1} / {heroList.length}
              </span>
              {heroList.map((_, idx) => {
                const isActive = idx === (((heroIndex % heroList.length) + heroList.length) % heroList.length);
                return (
                  <button
                    key={idx}
                    onClick={(e) => {
                      e.stopPropagation();
                      setHeroIndex(idx);
                    }}
                    className={`h-2 rounded-full transition-all duration-300 cursor-pointer ${
                      isActive ? "w-6 bg-[#FF6B00]" : "w-2 bg-white/30 hover:bg-white/60"
                    }`}
                    aria-label={`Go to slide ${idx + 1}`}
                  />
                );
              })}
            </div>
          )}

          {/* Bottom Left Content Area */}
          <div className="absolute bottom-12 left-12 w-full max-w-[70%] z-10 text-double-guard">
            {/* Dynamic Tag & Subtitle Pills */}
            <div className="flex items-center gap-2.5 mb-3 flex-wrap">
              <h2 className={`font-black tracking-[0.18em] text-[10px] uppercase inline-flex items-center gap-1.5 px-3 py-1 rounded-xl border backdrop-blur-md ${
                activeHero.spotlight_tag === "NEW EPISODE AVAILABLE" || activeHero.spotlight_tag === "NEW EPISODE RELEASED"
                  ? "border-[#FF6B00]/60 bg-[#FF6B00]/25 text-[#FF8533] shadow-[0_0_12px_rgba(255,107,0,0.25)]"
                  : activeHero.spotlight_tag === "SEASON FINALE"
                  ? "border-amber-500/40 bg-amber-500/20 text-amber-300 shadow-amber-500/20 shadow-sm"
                  : activeHero.spotlight_tag === "TOP RATED"
                  ? "border-yellow-500/40 bg-yellow-500/20 text-yellow-300"
                  : activeHero.spotlight_tag === "BINGE MOMENTUM"
                  ? "border-purple-500/40 bg-purple-500/20 text-purple-300"
                  : activeHero.spotlight_tag === "READY TO PLAY"
                  ? "border-emerald-500/40 bg-emerald-500/20 text-emerald-300"
                  : "border-[#FF6B00]/30 bg-black/50 text-[#FF6B00]"
              }`}>
                {activeHero.spotlight_tag === "NEW EPISODE AVAILABLE" || activeHero.spotlight_tag === "NEW EPISODE RELEASED" ? (
                  <Flame className="w-3.5 h-3.5 fill-[#FF6B00]" />
                ) : activeHero.spotlight_tag === "SEASON FINALE" ? (
                  <Star className="w-3.5 h-3.5 fill-amber-400" />
                ) : null}
                {activeHero.spotlight_tag || (activeHero.status === "Watching" && activeHero.last_position > 0 ? "Resume Watching" : activeHero.status === "Unwatched" ? "Start Series" : "Up Next")}
              </h2>

              {activeHero.spotlight_subtitle && (
                <span className="text-xs text-white/70 font-medium px-2.5 py-0.5 rounded-lg bg-black/40 border border-white/5 backdrop-blur-sm">
                  {activeHero.spotlight_subtitle}
                </span>
              )}
            </div>

            <h1 className="text-4xl md:text-5xl lg:text-6xl font-black text-white mb-2 tracking-tighter line-clamp-2 leading-tight [text-shadow:0_4px_12px_rgba(0,0,0,0.5)]">
              {activeHero.show_title || "Unknown Show"}
            </h1>
            <p className="text-lg text-[#A0AEC0] mb-6 truncate font-normal text-double-guard">
              {activeHero.media_type === "TV" ? (
                <>
                  {activeHero.season_num === 0 ? "SPECIAL" : `SEASON ${activeHero.season_num}`}
                  <span className="mx-2 opacity-30">•</span>
                  {`EPISODE ${activeHero.ep_num}${activeHero.title ? ` - ${activeHero.title}` : ''}`}
                  {activeHero.is_season_finale && (
                    <span className="ml-3 text-xs font-bold text-amber-400 bg-amber-400/10 px-2 py-0.5 rounded-md border border-amber-400/20 uppercase tracking-wider">
                      Season Finale
                    </span>
                  )}
                </>
              ) : (
                activeHero.title || "No Title"
              )}
            </p>

            <div className="flex items-center gap-4">
              <motion.button
                whileHover={isCinemaMode ? { scale: 1.05 } : {}}
                whileTap={isCinemaMode ? { scale: 0.95 } : {}}
                transition={isCinemaMode ? { type: "spring", stiffness: 400, damping: 10 } : { duration: 0 }}
                onClick={async () => {
                  if (isPlaybackActive) return;

                  if (!activeHero?.file_path) {
                    try {
                      const { open } = await import('@tauri-apps/plugin-dialog');
                      const selected = await open({
                        multiple: false,
                        title: `Locate File for ${activeHero?.show_title || 'Episode'}`,
                        filters: [{ name: 'Video Files', extensions: ['mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'm4v'] }]
                      });
                      if (selected && typeof selected === 'string' && activeHero) {
                        await invoke("update_local_file", { episodeId: activeHero.id, newPath: selected });
                        toast.success("File linked successfully!");
                        setIsPlaybackActive(true);
                        invoke("play_episode_cmd", {
                          episodeId: activeHero.id,
                          filePath: selected,
                          lastPosition: activeHero.last_position || 0,
                        }).catch(e => {
                          setIsPlaybackActive(false);
                          toast.error(`VLC Launch Failed: ${e}`);
                        });
                      }
                    } catch (err) {
                      toast.error("Failed to link file.");
                    }
                    return;
                  }

                  setIsPlaybackActive(true);
                  logger.click(`'Resume' on Hero (${activeHero.show_title} S${activeHero.season_num}E${activeHero.ep_num})`);
                  logger.ipcSend("play_episode_cmd", `Episode ${activeHero.id}`);
                  invoke("play_episode_cmd", {
                    episodeId: activeHero.id,
                    filePath: activeHero.file_path,
                    lastPosition: activeHero.last_position,
                  }).then(() => {
                    logger.ipcSuccess("VLC successfully launched. Waiting for heartbeat...");
                  }).catch(e => {
                    setIsPlaybackActive(false);
                    logger.error("VLC Launch Failed", e);
                    if (e === "VLC_AUTH_ERROR") {
                      toast.error("VLC Authentication Error: Failed to inject dynamic password or bind to port.", { duration: 8000 });
                    } else {
                      toast.error(`VLC Launch Failed: ${e}`);
                    }
                  });
                }}
                className={`flex items-center justify-center gap-2 px-8 py-3 rounded-xl font-bold uppercase tracking-wider transition-all duration-300 cursor-pointer ${
                  activeHero?.file_path
                    ? "bg-[#FF6B00] hover:bg-[#FF8533] text-white shadow-[0_4px_14px_0_rgba(255,107,0,0.39)]"
                    : "bg-[#FF6B00]/20 hover:bg-[#FF6B00]/40 text-orange-200 border border-[#FF6B00]/40 shadow-md"
                }`}
              >
                {activeHero?.file_path ? (
                  isPlaybackActive ? (
                    <>
                      <Loader2 className="w-5 h-5 animate-spin text-white" />
                      Now Playing
                    </>
                  ) : (
                    <>
                      <Icon icon={Play} fill="currentColor" className="w-5 h-5" />
                      {activeHero.status === "Watching" && activeHero.last_position > 0 ? "Resume" : "Play"}
                    </>
                  )
                ) : (
                  <>
                    <Icon icon={FolderOpen} className="w-5 h-5" />
                    Locate & Play
                  </>
                )}
              </motion.button>
              <button
                onClick={() => {
                    logger.click(`'More Info' for Hero (${activeHero.show_title})`);
                    onMediaSelect(activeHero.media_id);
                }}
                className="px-8 py-3 rounded-lg font-bold bg-white/10 hover:bg-white/20 text-white backdrop-blur-md transition-all duration-300 hover:scale-105"
              >
                More Info
              </button>
            </div>

            {/* Progress Bar & Time Display (with 90% color shift and hover reveal) */}
            {activeHero.status === "Watching" && activeHero.runtime > 0 && (
              <div className="flex flex-col mt-4">
                <div className="flex items-center gap-3 mb-2">
                  <div className="text-sm font-bold text-white">
                    {heroRemainingTime}
                  </div>
                  {showProgressTooltip && (
                    <span className="text-xs font-mono font-semibold text-white/90 bg-black/75 px-2 py-0.5 rounded-md border border-white/10 shadow-sm">
                      {heroTimestampDisplay}
                    </span>
                  )}
                </div>
                <div
                  onMouseEnter={() => setShowProgressTooltip(true)}
                  onMouseLeave={() => setShowProgressTooltip(false)}
                  className="w-64 h-2 bg-white/20 overflow-hidden flex rounded-full cursor-pointer relative group/prog"
                  title={heroTimestampDisplay}
                >
                  {(() => {
                    const progress = calculateProgress(activeHero.last_position, activeHero.runtime);
                    if (progress <= 0) return null;
                    const isOver90 = progress >= 90;
                    return (
                      <div
                        className={`h-full transition-all duration-300 rounded-full ${
                          isOver90 ? "bg-[#22C55E] shadow-[0_0_10px_rgba(34,197,94,0.5)]" : "bg-[#FF6B00]"
                        }`}
                        style={{
                          width: `${progress}%`,
                        }}
                      />
                    );
                  })()}
                </div>
              </div>
            )}
          </div>
        </div>
      ) : (
        <div className="relative w-full h-[450px] min-h-[400px] lg:h-[50vh] overflow-hidden rounded-3xl bg-[#1F222A]/80 backdrop-blur-xl flex flex-col items-center justify-center text-center mb-10">
          <h1 className="text-4xl font-bold mb-4 text-white">Welcome to WatchMark</h1>
          <p className="text-muted max-w-md">Scan your local folder or search TMDB to get started and build your library.</p>
        </div>
      )}

      {/* Upcoming Airings (Next 7 Days) */}
      <UpcomingAiringRow
        airings={filteredAirings}
        onSelectMedia={onMediaSelect}
      />

      {/* B. Continue Watching (Horizontal Row) */}
      {filteredCW.length > 0 && (
        <div className="relative group/cw">
          <h2 className="text-2xl font-bold mt-12 mb-6 text-white">Continue Watching</h2>

          <button
              onClick={cwScroll.scrollLeft}
              className="absolute left-0 top-1/2 -translate-y-1/2 -ml-4 z-40 p-3 bg-black/40 backdrop-blur-md hover:bg-black/60 rounded-full text-white opacity-0 group-hover/cw:opacity-100 transition-opacity"
              aria-label="Scroll left"
          >
              <ChevronLeft className="w-8 h-8" />
          </button>
          <button
              onClick={cwScroll.scrollRight}
              className="absolute right-0 top-1/2 -translate-y-1/2 -mr-4 z-40 p-3 bg-black/40 backdrop-blur-md hover:bg-black/60 rounded-full text-white opacity-0 group-hover/cw:opacity-100 transition-opacity"
              aria-label="Scroll right"
          >
              <ChevronRight className="w-8 h-8" />
          </button>

          <motion.div ref={cwScroll.elRef} layout className={`flex gap-6 overflow-x-auto pb-4 scrollbar-hide snap-x snap-mandatory pr-[20%] ${filteredCW.length <= 2 ? 'justify-start max-w-full' : ''}`}>
            <AnimatePresence mode="popLayout">
            {filteredCW.map((ep: any) => {
              const progress = calculateProgress(ep.last_position, ep.runtime);

              const stillUrl = ep.still_path ? formatImagePath(ep.still_path, "w500") : "";
              const fallbackUrl = ep.backdrop_path ? formatImagePath(ep.backdrop_path, "w500") : PLACEHOLDER_BACKDROP;

              return (
                <motion.div
                  key={ep.id}
                  layout="position"
                  initial={isCinemaMode ? { opacity: 0, scale: 0.9 } : { opacity: 1, scale: 1 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={isCinemaMode ? { opacity: 0, scale: 0.9, width: 0, marginLeft: -24 } : { opacity: 0 }}
                  transition={isCinemaMode ? { type: "spring", stiffness: 300, damping: 30 } : { duration: 0 }}
                  onClick={() => {
                      logger.click(`Continue Watching: ${ep.show_title} (Media ID: ${ep.media_id})`);
                      onMediaSelect(ep.media_id);
                  }}
                  whileHover={isCinemaMode ? { scale: 1.02 } : {}}
                  className="flex-none min-w-[320px] bg-[#1F222A] rounded-xl overflow-hidden cursor-pointer group transition-all duration-300 hover:ring-2 hover:ring-[#FF6B00]/50 snap-start shadow-lg relative transform-gpu focus:outline-none focus:ring-2 focus:ring-[#FF6B00] focus:ring-offset-2 focus:ring-offset-[#0D0F14]"
                >
                  <div className="w-full aspect-video relative overflow-hidden bg-[#1F222A]">
                    {/* Render Image with blur fallback logic built-in to safeImage for still types, but let's ensure styling per 8.12 */}
                    {!ep.still_path ? (
                      <div className="relative w-full h-full">
                        <SafeImage
                          srcPath={fallbackUrl}
                          type="backdrop"
                          altText={ep.show_title || "Show Thumbnail"}
                          className="w-full h-full object-cover filter blur-[15px] brightness-50"
                        />
                        <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
                           <span className="text-3xl font-bold text-white drop-shadow-md tracking-widest">EP {ep.ep_num}</span>
                        </div>
                      </div>
                    ) : (
                      <SafeImage
                        srcPath={stillUrl}
                        fallbackSrcPath={fallbackUrl}
                        type="still"
                        episodeNumber={ep.ep_num}
                        title={ep.show_title || "Unknown Show"}
                        altText={ep.show_title || "Show Thumbnail"}
                        isFallbackImage={ep.is_fallback_image}
                        potentialSpoiler={ep.potential_spoiler}
                        isCompleted={ep.status === "Completed"}
                        className="w-full h-full object-cover opacity-80 group-hover:opacity-100 transition-all duration-500 group-hover:scale-105"
                      />
                    )}

                    <div className="absolute inset-0 bg-gradient-to-t from-black/80 via-transparent to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-300 pointer-events-none" />

                    <button
                      onClick={async (e) => {
                        e.stopPropagation();
                        if (!ep.file_path) {
                          try {
                            const { open } = await import('@tauri-apps/plugin-dialog');
                            const selected = await open({
                              multiple: false,
                              title: `Locate File for ${ep.show_title || 'Episode'}`,
                              filters: [{ name: 'Video Files', extensions: ['mp4', 'mkv', 'avi', 'mov', 'wmv', 'flv', 'webm', 'm4v'] }]
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
                          return;
                        }

                        logger.click(`Play on CW item (${ep.show_title} S${ep.season_num}E${ep.ep_num})`);
                        logger.ipcSend("play_episode_cmd", `Episode ${ep.id}`);
                        invoke("play_episode_cmd", {
                          episodeId: ep.id,
                          filePath: ep.file_path,
                          lastPosition: ep.last_position,
                        }).then(() => {
                           logger.ipcSuccess("VLC successfully launched. Waiting for heartbeat...");
                        }).catch(err => {
                           logger.error("VLC Launch Failed", err);
                           if (err === "VLC_AUTH_ERROR") {
                             toast.error("VLC Authentication Error: Failed to inject dynamic password or bind to port.", { duration: 8000 });
                           } else {
                             toast.error(`VLC Launch Failed: ${err}`);
                           }
                        });
                      }}
                      title={ep.file_path ? "Play in VLC" : "Locate & Play Video File"}
                      className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-12 h-12 bg-[#FF6B00] text-white rounded-full flex items-center justify-center opacity-0 group-hover:opacity-100 transition-all scale-75 group-hover:scale-100 z-20 hover:bg-[#E66000]"
                    >
                      <Icon icon={Play} fill="currentColor" className="w-5 h-5 ml-1" />
                    </button>
                  </div>

                  <div className="absolute bottom-0 left-0 w-full h-1 z-10">
                    {progress < 5 ? (
                      <div className="h-full w-full" style={{ backgroundColor: "rgba(255, 255, 255, 0.1)" }} />
                    ) : (
                      <div className="h-full bg-white/10 w-full flex">
                        <div
                          className="h-full"
                          style={{
                            width: `${progress}%`,
                            backgroundColor: progress >= 90 ? "#1b5e20" : "#FF6B00"
                          }}
                        />
                      </div>
                    )}
                  </div>

                  <div className="p-4 flex justify-between items-center bg-[#1F222A] relative z-20">
                    <h3 className="font-bold text-white truncate pr-2">{ep.show_title || "Unknown Show"}</h3>
                    <span className="text-xs font-normal text-muted whitespace-nowrap tabular-nums">
                      {ep.media_type === "TV" ? `S${ep.season_num}E${ep.ep_num}` : (ep.title || "No Title")}
                    </span>
                  </div>
                </motion.div>
              );
            })}
            </AnimatePresence>
          </motion.div>
        </div>
      )}

      {/* C. Recently Added (Poster Grid Row) */}
      {filteredRecent.length > 0 && (
        <div className="relative group/recent">
          <h2 className="text-2xl font-bold mt-12 mb-6 text-white">Recently Added</h2>

          <button
              onClick={recentScroll.scrollLeft}
              className="absolute left-0 top-1/2 -translate-y-1/2 -ml-4 z-40 p-3 bg-black/40 backdrop-blur-md hover:bg-black/60 rounded-full text-white opacity-0 group-hover/recent:opacity-100 transition-opacity"
              aria-label="Scroll left"
          >
              <ChevronLeft className="w-8 h-8" />
          </button>
          <button
              onClick={recentScroll.scrollRight}
              className="absolute right-0 top-1/2 -translate-y-1/2 -mr-4 z-40 p-3 bg-black/40 backdrop-blur-md hover:bg-black/60 rounded-full text-white opacity-0 group-hover/recent:opacity-100 transition-opacity"
              aria-label="Scroll right"
          >
              <ChevronRight className="w-8 h-8" />
          </button>

          <motion.div ref={recentScroll.elRef} layout className="flex gap-4 overflow-x-auto pb-8 scrollbar-hide snap-x snap-mandatory pt-2 pr-[20%]">
            <AnimatePresence mode="popLayout">
            {filteredRecent.map((media: any) => {
              const posterUrl = media.poster_path ? formatImagePath(media.poster_path, "w500") : "";

              return (
                <motion.div
                  key={media.id}
                  layout="position"
                  initial={isCinemaMode ? { opacity: 0, scale: 0.9 } : { opacity: 1, scale: 1 }}
                  animate={{ opacity: 1, scale: 1 }}
                  exit={isCinemaMode ? { opacity: 0, scale: 0.9, width: 0, marginLeft: -16 } : { opacity: 0 }}
                  transition={isCinemaMode ? { type: "spring", stiffness: 300, damping: 30 } : { duration: 0 }}
                  onClick={() => {
                      logger.click(`Recent Additions: ${media.title} (Media ID: ${media.id})`);
                      onMediaSelect(media.id);
                  }}
                  whileHover={isCinemaMode ? { scale: 1.05, zIndex: 10 } : {}}
                  className="relative flex-none w-[140px] md:w-[160px] lg:w-[180px] aspect-[2/3] rounded-2xl overflow-hidden cursor-pointer group snap-start shadow-xl transition-shadow duration-300 hover:shadow-2xl hover:shadow-[#FF6B00]/10 bg-[#1F222A] transform-gpu focus:outline-none focus:ring-2 focus:ring-[#FF6B00] focus:ring-offset-2 focus:ring-offset-[#0D0F14]"
                >
                  <SafeImage
                    srcPath={posterUrl}
                    fallbackSrcPath={PLACEHOLDER_POSTER}
                    type="poster"
                    altText={media.title || "Unknown Title"}
                    title={media.title}
                    className="w-full h-full object-cover"
                  />

                  <div className="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 transition-opacity duration-200 flex items-center justify-center backdrop-blur-sm">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        logger.click(`Play Next on Recent (${media.title})`);
                        invoke("play_next_episode_cmd", { mediaId: media.id }).then(() => {
                           logger.ipcSuccess("VLC Play Next launched");
                        }).catch(err => {
                           toast.error(`Play Next Failed: ${err}`);
                        });
                      }}
                      className="w-14 h-14 bg-[#FF6B00] text-white rounded-full flex items-center justify-center shadow-lg shadow-[#FF6B00]/30 scale-75 group-hover:scale-100 transition-all duration-300 hover:scale-110 hover:bg-[#E66000]"
                    >
                      <Icon icon={Play} fill="currentColor" className="w-6 h-6 ml-1" />
                    </button>
                  </div>

                  {/* Always-on Top-Right Star Rating pill */}
                  {media.user_rating !== null && media.user_rating > 0 && (
                    <div className="absolute top-2 right-2 bg-[#0D0F14]/60 backdrop-blur-md px-2 py-1 rounded-xl text-xs font-bold text-white shadow-md flex items-center gap-1 tabular-nums">
                       <Star className="w-3 h-3 fill-[#FF6B00] text-[#FF6B00]" /> {media.user_rating}
                    </div>
                  )}

                  {media.total_episodes > 0 && (
                    <div className="absolute bottom-0 left-0 w-full h-1 bg-white/20">
                      <div
                        className="h-full bg-green-500"
                        style={{ width: `${Math.min(100, (media.completed_eps / media.total_episodes) * 100)}%` }}
                      />
                    </div>
                  )}
                </motion.div>
              );
            })}
            </AnimatePresence>
          </motion.div>
        </div>
      )}

      {/* D. Quick Stats */}
      <h2 className="text-2xl font-bold mt-12 mb-6 text-white">Your Stats</h2>
      <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-12">
        <div className="bg-[#1F222A]/40 backdrop-blur-md rounded-2xl p-6 border border-white/5 transition-all duration-300 hover:bg-[#1F222A]/60 shadow-xl flex flex-col items-center justify-center text-center">
          <h3 className="text-sm font-normal text-muted uppercase tracking-widest mb-2">Shows Tracked</h3>
          <p className="text-5xl font-black text-[#FF6B00] drop-shadow-md tabular-nums">{data.stats.shows_completed}</p>
        </div>
        <div className="bg-[#1F222A]/40 backdrop-blur-md rounded-2xl p-6 border border-white/5 transition-all duration-300 hover:bg-[#1F222A]/60 shadow-xl flex flex-col items-center justify-center text-center">
          <h3 className="text-sm font-normal text-muted uppercase tracking-widest mb-2">Hours Watched</h3>
          <p className="text-5xl font-black text-[#FF6B00] drop-shadow-md tabular-nums">{data.stats.hrs_watched}</p>
        </div>
        <div className="bg-[#1F222A]/40 backdrop-blur-md rounded-2xl p-6 border border-white/5 transition-all duration-300 hover:bg-[#1F222A]/60 shadow-xl flex flex-col items-center justify-center text-center">
          <h3 className="text-sm font-normal text-muted uppercase tracking-widest mb-2">Average Rating</h3>
          <p className="text-5xl font-black text-[#FF6B00] drop-shadow-md flex items-center justify-center gap-2 tabular-nums">
            <Icon icon={Star} className="w-8 h-8 fill-current" /> {data.stats.avg_rating.toFixed(1)}
          </p>
        </div>
      </div>
    </div>
  );
}
