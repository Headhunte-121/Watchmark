import { useState, useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { LayoutDashboard, Tv, Film, Search, Inbox, Clock, Settings, Menu, XCircle } from "lucide-react";
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";
import { motion, AnimatePresence, MotionConfig } from "framer-motion";
import { useAppStore } from "./store/useAppStore";
import { useTaskStore } from "./store/useTaskStore";
import Dashboard from "./components/Dashboard";
import { toast } from "./utils/toast";
import Library from "./components/Library";
import SearchTMDB from "./components/SearchTMDB";
import History from "./components/History";
import InboxView from "./components/InboxView";
import SettingsView from "./components/SettingsView";
import MediaDetails from "./components/MediaDetails";
import { Toaster } from "sonner";
import { Modal } from "./components/ui/Modal";
import { ProcessingModal } from "./components/ui/ProcessingModal";
import { OptimizationModal } from "./components/ui/OptimizationModal";
import { useAsyncInvoke } from "./hooks/useAsyncInvoke";
import { logger } from "./utils/logger";
import { Icon } from "./components/ui/Icon";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

export type View = "dashboard" | "tv" | "movies" | "search" | "inbox" | "history" | "settings";

function App() {
  const [currentView, setCurrentView] = useState<View>("dashboard");
  const [selectedMediaId, setSelectedMediaId] = useState<number | null>(null);
  const [selectedSeasonNum, setSelectedSeasonNum] = useState<number | undefined>(undefined);
  const asyncInvoke = useAsyncInvoke();

  // Settings Navigation Guard
  const [isSettingsDirty, setIsSettingsDirty] = useState(false);
  const [pendingNavigation, setPendingNavigation] = useState<{ view: View | null, mediaId: number | null, seasonNum?: number } | null>(null);
  const [saveSettingsCallback, setSaveSettingsCallback] = useState<(() => Promise<boolean>) | null>(null);

  // Refresh UI hook
  const [refreshTrigger, setRefreshTrigger] = useState(0);
  const [isMenuOpen, setIsMenuOpen] = useState(false);
  const [isHeaderScrolled, setIsHeaderScrolled] = useState(false);
  const { isCinemaMode, initialized, initializeSettings, setApiAuthorized, searchQuery: globalSearchQuery, setSearchQuery: setGlobalSearchQuery } = useAppStore();

  const isImporting = useTaskStore((state) => state.isImporting);
  const importProgress = useTaskStore((state) => state.progress);
  const importTotal = useTaskStore((state) => state.total);
  const activeSyncs = useTaskStore((state) => state.activeSyncs);

  // Focus state for applying native signal (grayscale/opacity) when out of focus
  const [isFocused, setIsFocused] = useState(true);

  const { isOffline, setOffline } = useAppStore();

  useEffect(() => {
    // Check initial online status
    setOffline(!navigator.onLine);

    let debounceTimer: ReturnType<typeof setTimeout> | null = null;
    const handleStatusChange = (online: boolean) => {
      if (debounceTimer) clearTimeout(debounceTimer);
      debounceTimer = setTimeout(() => {
        setOffline(!online);
      }, 3000);
    };

    const onOnline = () => handleStatusChange(true);
    const onOffline = () => handleStatusChange(false);

    window.addEventListener("online", onOnline);
    window.addEventListener("offline", onOffline);

    const unlistenHeartbeat = listen("network-status", (event: any) => {
      const isOnline = event.payload.online;
      handleStatusChange(isOnline);
    });

    return () => {
      window.removeEventListener("online", onOnline);
      window.removeEventListener("offline", onOffline);
      unlistenHeartbeat.then(fn => fn());
      if (debounceTimer) clearTimeout(debounceTimer);
    };
  }, [setOffline]);

  useEffect(() => {
    logger.app("WatchMark Frontend successfully mounted.");
    initializeSettings();
    useAppStore.getState().prefetchHistory();

    const handleWindowScroll = () => {
        setIsHeaderScrolled(window.scrollY > 10);
    };

    // Add global scroll listener to window
    window.addEventListener("scroll", handleWindowScroll);

    // Setup dynamic font
    import("@tauri-apps/api/core").then(({ invoke, convertFileSrc }) => {
        invoke<string>("get_app_data_dir").then(appDataDir => {
            // Convert backslashes for windows paths to avoid regex parsing issues
            const cleanPath = appDataDir.replace(/\\/g, '/');
            const fontPath = `${cleanPath}/fonts/Inter-Variable.woff2`;
            const assetUrl = convertFileSrc(fontPath);

            // Inject global font-face
            const style = document.createElement('style');
            style.innerHTML = `
                @font-face {
                    font-family: 'Inter';
                    font-style: normal;
                    font-weight: 100 900;
                    font-display: swap;
                    src: url('${assetUrl}') format('woff2');
                }
            `;
            document.head.appendChild(style);
        }).catch(e => {
            console.error("Failed to fetch AppData dir for fonts:", e);
        });
    });

    // Check for successful restore flag
    if (localStorage.getItem('restore_success') === 'true') {
        localStorage.removeItem('restore_success');

        // Fetch fresh stats to show in the toast
        import("@tauri-apps/api/core").then(({ invoke }) => {
            Promise.all([
                asyncInvoke("get_library_data"),
                invoke("get_media_history_count")
            ]).then(([library, historyCount]: [any, any]) => {
                const totalShows = library?.tv_shows?.length || 0;
                const totalMovies = library?.movies?.length || 0;
                const totalItems = totalShows + totalMovies;
                const hCount = typeof historyCount === 'number' ? historyCount : 0;

                toast.success(`Library Restored! ${totalItems} shows and ${hCount} history entries recovered.`, { duration: 5000 });
                setRefreshTrigger(prev => prev + 1);
            }).catch(e => {
                console.error("Failed to fetch restored stats:", e);
                toast.success("Library Restored Successfully!");
                setRefreshTrigger(prev => prev + 1);
            });
        }).catch(e => console.error("Failed to import invoke:", e));
    }

    return () => {
        window.removeEventListener("scroll", handleWindowScroll);
    };
  }, [initializeSettings]);

  useEffect(() => {
    const unlisten = listen("vlc-closed", () => {
      setRefreshTrigger(prev => prev + 1);
      useAppStore.getState().bumpHistoryVersion();
    });

    const unlistenSessionEnded = listen("vlc-session-ended", () => {
      // 4.16.2 Global UI Invalidation (No-Refresh Sync)
      setRefreshTrigger(prev => prev + 1);
      useAppStore.getState().bumpHistoryVersion();
    });

    // Removals made from the History tab: refresh dashboard stats only.
    // The History tab already reflects the change locally.
    const unlistenHistoryChanged = listen("history-changed", () => {
      setRefreshTrigger(prev => prev + 1);
    });

    const unlistenMediaDeleted = listen("media-deleted", () => {
      useAppStore.getState().bumpHistoryVersion();
    });

    const unlistenResize = listen("tauri://resize", () => {
      // Trigger a forced reflow pass on window resize or restore
      // By slightly mutating the refresh trigger, we force components like Library and Intersection Observers to recalculate
      window.dispatchEvent(new Event('resize'));
    });

    const unlistenProgress = listen<{ progress: number; total: number; isImporting: boolean }>(
      "history-import-progress",
      (event) => {
        useTaskStore.getState().setProgress(event.payload.progress, event.payload.total, event.payload.isImporting);
        if (!event.payload.isImporting) {
            setRefreshTrigger(prev => prev + 1);
            useAppStore.getState().bumpHistoryVersion();
        }
      }
    );

    const unlistenSyncProgress = listen<{ mediaId: number; tmdbId?: string; currentSeason: number; totalSeasons: number }>(
      "sync-progress",
      (event) => {
        const { mediaId, tmdbId, currentSeason, totalSeasons } = event.payload;
        useTaskStore.getState().updateSyncProgress(mediaId.toString(), currentSeason, totalSeasons);
        if (tmdbId) {
            useTaskStore.getState().updateSyncProgress(`tmdb_${tmdbId}`, currentSeason, totalSeasons);
        }
      }
    );

    const unlistenDbFailed = listen("db-write-failed", () => {
        toast.error(
            "Database Sync Issue: A change failed to save after 3 attempts. Your data is safe, but this action needs a retry.",
            { duration: 8000, style: { background: "#b71c1c", color: "#ffffff", border: "none" } }
        );
    });

    const unlistenApiFailed = listen("api-auth-failed", () => {
        setApiAuthorized(false);
        toast.error(
            "API Key Unauthorized. All background metadata requests have been halted. Please check your Settings.",
            { duration: 8000, style: { background: "#b71c1c", color: "#ffffff", border: "none" } }
        );
    });

    const unlistenFocus = listen("tauri://focus", () => {
      setIsFocused(true);
    });

    const unlistenBlur = listen("tauri://blur", () => {
      setIsFocused(false);
    });

    const unlistenTrayScan = listen("tray-scan", async () => {
      try {
        const { open } = await import('@tauri-apps/plugin-dialog');
        const { invoke } = await import('@tauri-apps/api/core');
        const { invokeWithTimeout } = await import('./utils/ipc');

        const selected = await open({
          directory: true,
          multiple: false,
          title: "Select Directory to Scan"
        });

        if (selected && typeof selected === "string") {
          toast.success("Scanning...", { description: `Scanning ${selected}` });
          useTaskStore.getState().setScanning(true);
          try {
            const res = await invokeWithTimeout<any>("run_scan_directory", { directory: selected }, 300000);
            toast.scanComplete(res);
            const settings: any = await invoke('get_settings');
            settings.last_scanned_path = selected;
            await invoke('save_settings', { settings });
          } catch (e: any) {
             logger.error("Scan Failed", e);
             if (e?.type === "AccessDenied" || e?.code === "ACCESS_DENIED") {
               toast.error(`Access Denied: WatchMark lacks permissions for ${e.path}`);
             } else {
               toast.error("Error during scan: " + e);
             }
          } finally {
             useTaskStore.getState().setScanning(false);
          }
        }
      } catch (err) {
        console.error("Failed to open dialog or run scan", err);
      }
    });

    const unlistenCheckUpdates = listen("check-for-updates", () => {
      toast.info("Checking for updates...");
      // For now just simulate it, or if there is an updater plugin, call it.
      setTimeout(() => {
        toast.success("WatchMark is up to date!");
      }, 1500);
    });

    return () => {
      unlisten.then(fn => fn());
      unlistenSessionEnded.then(fn => fn());
      unlistenHistoryChanged.then(fn => fn());
      unlistenMediaDeleted.then(fn => fn());
      unlistenResize.then(fn => fn());
      unlistenProgress.then(fn => fn());
      unlistenSyncProgress.then(fn => fn());
      unlistenDbFailed.then(fn => fn());
      unlistenApiFailed.then(fn => fn());
      unlistenFocus.then(fn => fn());
      unlistenBlur.then(fn => fn());
      unlistenTrayScan.then(fn => fn());
      unlistenCheckUpdates.then(fn => fn());
    };
  }, []);

  const { isScanning } = useTaskStore();

  useEffect(() => {
    document.documentElement.setAttribute('data-cinema-mode', isCinemaMode.toString());
  }, [isCinemaMode]);

  // WARNING: DO NOT PLACE ANY HOOKS (useState, useEffect, useMemo, etc.) BELOW THIS LINE.
  // ALL HOOKS MUST BE DECLARED ABOVE THIS EARLY RETURN TO PREVENT RULES-OF-HOOKS VIOLATIONS.
  if (!initialized) {
    return null; // or a simple spinner
  }

  const navItems = [
    { id: "dashboard", label: "Dashboard", icon: LayoutDashboard },
    { id: "tv", label: "TV Shows", icon: Tv },
    { id: "movies", label: "Movies", icon: Film },
    { id: "search", label: "Search", icon: Search },
    { id: "inbox", label: "Inbox", icon: Inbox },
    { id: "history", label: "History", icon: Clock },
  ] as const;

  const handleNav = (view: View, mediaId: number | null = null, seasonNum?: number) => {
    if (isSettingsDirty && currentView === "settings" && view !== "settings") {
      setPendingNavigation({ view, mediaId, seasonNum });
    } else {
      // Allow re-clicking the same tab to close the current item
      if (currentView === view && !selectedMediaId && !mediaId) {
          return; // Do nothing if already on the root of the tab
      }

      if (mediaId && mediaId !== selectedMediaId) {
          logger.navTo(`Media Details for ID: ${mediaId}`);
      } else if (!mediaId && selectedMediaId) {
          logger.navBack(`Return to ${currentView}`);
      } else if (view !== currentView) {
          logger.navTo(`'${navItems.find(i => i.id === view)?.label || view}' Tab`);
      }

      // 13.1 Dedicated 'History' sidebar routing tab View Reset command
      if (view === "history") {
          setGlobalSearchQuery("");
      }

      setCurrentView(view);
      setSelectedMediaId(mediaId);
      setSelectedSeasonNum(seasonNum);
    }
  };

  const confirmNavigation = async (save: boolean) => {
    if (save && saveSettingsCallback) {
        const success = await saveSettingsCallback();
        if (!success) return; // Keep modal open or stay on page if save fails
    }

    // We must reset the unsaved changes state in the components as well
    // but React state will reset when the view is unmounted.
    setIsSettingsDirty(false);
    if (pendingNavigation) {
        if (pendingNavigation.view) {
            setCurrentView(pendingNavigation.view);
        }
        setSelectedMediaId(pendingNavigation.mediaId);
        setSelectedSeasonNum(pendingNavigation.seasonNum);
        setPendingNavigation(null);
    }
  };

  const cancelNavigation = () => {
      setPendingNavigation(null);
  };




  return (
    <MotionConfig transition={isCinemaMode ? { type: "spring", stiffness: 300, damping: 30 } : { duration: 0 }}>
    <div className="h-screen overflow-hidden flex bg-cinema-black text-white selection:bg-brand-orange/30">

      {/* Visual scan activity indicator */}
      {isScanning && (
        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          className="fixed top-0 left-0 right-0 h-1 bg-[#FF6B00] z-[9999] overflow-hidden"
        >
          <motion.div
            className="h-full w-1/3 bg-white/50 blur-[2px]"
            animate={{ x: ["-100%", "400%"] }}
            transition={{ repeat: Infinity, duration: 1.5, ease: "linear" }}
          />
        </motion.div>
      )}

      {/* Mobile Scrim */}
      <AnimatePresence>
        {isMenuOpen && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            onClick={() => setIsMenuOpen(false)}
            className="fixed inset-0 z-40 bg-black/60 backdrop-blur-sm"
          />
        )}
      </AnimatePresence>

      {/* Glassy Sidebar */}
      <motion.aside
        initial={false}
        animate={{ x: isMenuOpen ? 0 : "-100%" }}
        transition={isCinemaMode ? { type: "spring", stiffness: 400, damping: 30 } : { duration: 0 }}
        className={cn(
          "fixed z-50 h-full flex flex-col sidebar-parent",
          "w-64 min-w-[256px] max-w-[256px]",
          "bg-[#141519]/60 backdrop-blur-xl border-r border-white/5",
          !isFocused ? "grayscale-[20%] opacity-90" : "grayscale-0 opacity-100"
        )}
      >
        <div className="h-16 flex-shrink-0 flex items-center justify-center py-8">
          <button
            onClick={() => {
              handleNav("dashboard");
              setIsMenuOpen(false);
              document.querySelector('main > div.flex-1')?.scrollTo({ top: 0, behavior: 'smooth' });
            }}
            className="focus:outline-none"
          >
            <h1 className="text-2xl font-bold tracking-tight text-white flex items-center gap-2">
              <svg viewBox="0 0 32 32" className="w-8 h-8 drop-shadow-[0_0_8px_rgba(255,107,0,0.4)]">
                <path d="M10 6L26 16L10 26V6Z" fill="#FF6B00" />
              </svg>
              WatchMark
            </h1>
          </button>
        </div>

        <nav className="flex-1 px-4 space-y-1 overflow-y-auto scrollbar-hide sidebar-scroll-container">
          {navItems.map((item) => (
            <button
              key={item.id}
                onClick={() => { handleNav(item.id); setIsMenuOpen(false); }}
              className={cn(
                "w-full flex items-center gap-3 px-4 py-3 rounded-lg text-sm font-normal transition-all duration-200 relative overflow-hidden",
                currentView === item.id && !selectedMediaId
                  ? "text-white bg-gradient-to-r from-white/5 to-transparent"
                  : "text-muted hover:text-white hover:bg-white/5"
              )}
            >
              {currentView === item.id && !selectedMediaId && (
                <div className="absolute left-0 top-0 bottom-0 w-1 bg-[#FF6B00] shadow-[0_0_10px_#FF6B00]" />
              )}
              <Icon icon={item.icon} className={cn("w-5 h-5", currentView === item.id && !selectedMediaId && "text-[#FF6B00]")} />
              {item.label}
            </button>
          ))}
        </nav>

        <div className="p-4 mt-auto shrink-0">
          <button
            onClick={() => { handleNav("settings"); setIsMenuOpen(false); }}
            className={cn(
              "w-full flex items-center gap-3 px-4 py-3 rounded-lg text-sm font-normal transition-all duration-200 relative overflow-hidden",
              currentView === "settings" && !selectedMediaId
                ? "text-white bg-gradient-to-r from-white/5 to-transparent"
                : "text-muted hover:text-white hover:bg-white/5"
            )}
          >
            {currentView === "settings" && !selectedMediaId && (
              <div className="absolute left-0 top-0 bottom-0 w-1 bg-[#FF6B00] shadow-[0_0_10px_#FF6B00]" />
            )}
            <Icon icon={Settings} className={cn("w-5 h-5", currentView === "settings" && !selectedMediaId && "text-[#FF6B00]")} />
            Settings
          </button>
        </div>
      </motion.aside>

      {/* Main Content Area */}
      <main className="flex-1 flex flex-col relative overflow-hidden min-w-0 w-full pl-0 lg:pl-0">
        <Modal />
        <ProcessingModal />
        <OptimizationModal />

        {/* Progress Bar (Global) */}
        <div className="absolute top-0 left-0 w-full z-[100] pointer-events-none flex flex-col">
          <AnimatePresence>
            {isImporting && (
              <motion.div
                initial={{ opacity: 0, scaleX: 0 }}
                animate={{ opacity: 1, scaleX: importTotal > 0 ? importProgress / 100 : 0 }}
                exit={{ opacity: 0 }}
                style={{ originX: 0 }}
                transition={{ duration: 0.3, ease: "easeInOut" }}
                className="h-1 bg-[#FF6B00] shadow-[0_0_10px_#FF6B00] w-full origin-left"
              />
            )}
          </AnimatePresence>
          {Object.keys(activeSyncs).length > 0 && (
             <div className="h-1 bg-[#FF6B00]/80 w-full animate-pulse shadow-[0_0_10px_#FF6B00]" />
          )}
        </div>

        {Object.keys(activeSyncs).length > 0 && (
          <div className="absolute top-4 left-1/2 -translate-x-1/2 z-[100] bg-[#FF6B00]/20 backdrop-blur-md border border-[#FF6B00]/50 text-[#FF6B00] text-xs font-bold px-4 py-1.5 rounded-full shadow-lg shadow-orange-500/20 whitespace-nowrap animate-pulse mt-16">
            Syncing {Object.keys(activeSyncs).filter(k => !k.startsWith('tmdb_')).length} item{Object.keys(activeSyncs).filter(k => !k.startsWith('tmdb_')).length !== 1 ? "s" : ""}...
          </div>
        )}

        {/* Top Global Navigation Bar */}
        {/* Offline Banner */}
        <AnimatePresence>
          {isOffline && (
            <motion.div
              initial={{ y: -50, opacity: 0 }}
              animate={{ y: 0, opacity: 1 }}
              exit={{ y: -50, opacity: 0 }}
              className="absolute top-16 left-0 right-0 z-30 bg-amber-500/20 text-amber-400 py-1.5 flex items-center justify-center gap-2 text-sm font-bold shadow-lg"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><line x1="2" y1="2" x2="22" y2="22"></line><path d="M16.72 11.06A10.94 10.94 0 0 1 19 12.55"></path><path d="M5 12.55a10.94 10.94 0 0 1 5.17-2.39"></path><path d="M10.71 5.05A16 16 0 0 1 22.58 9"></path><path d="M1.42 9a15.91 15.91 0 0 1 4.7-2.88"></path><path d="M8.53 16.11a6 6 0 0 1 6.95 0"></path><line x1="12" y1="20" x2="12.01" y2="20"></line></svg>
              Offline Mode
            </motion.div>
          )}
        </AnimatePresence>

        <header
          data-tauri-drag-region
          className={cn(
            "absolute top-0 left-0 right-0 z-40 h-16 flex-shrink-0 flex items-center justify-between px-4 transition-all duration-300 border-b border-white/5 select-none",
            isHeaderScrolled ? "bg-[#0D0F14]/80 backdrop-blur-md" : "bg-transparent"
          )}
        >
          <div className="flex items-center z-10">
            {/* Hamburger Menu for Mobile */}
            <button
              onClick={() => setIsMenuOpen(true)}
              className="p-2 mr-2 text-white/70 hover:text-white focus:outline-none"
            >
              <Icon icon={Menu} className="w-6 h-6" />
            </button>
            {selectedMediaId && (
              <button
                onClick={() => handleNav(currentView)}
                className="ml-8 px-4 py-2 text-sm font-normal text-white/70 hover:text-white bg-white/5 hover:bg-white/10 rounded-lg transition-colors flex items-center gap-2"
              >
                ← Back
              </button>
            )}
            {currentView === "search" && !selectedMediaId && (
              <button
                onClick={() => handleNav("dashboard")}
                className="ml-8 px-4 py-2 text-sm font-normal text-white/70 hover:text-white bg-white/5 hover:bg-white/10 rounded-lg transition-colors flex items-center gap-2"
              >
                ← Back to Home
              </button>
            )}
          </div>

          {/* Quick Search Pill */}
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (currentView !== "search") {
                handleNav("search");
              }
            }}
            className="flex-1 max-w-xl px-4 mx-auto hidden md:block z-10"
          >
            <div className="relative">
              <Icon icon={Search} className="absolute left-4 top-1/2 -translate-y-1/2 text-gray-500 w-4 h-4" />
              <input
                type="text"
                placeholder="Quick Search..."
                value={globalSearchQuery}
                onChange={(e) => setGlobalSearchQuery(e.target.value)}
                id="global-search-input"
                className="w-full bg-[#1F222A]/80 backdrop-blur-xl text-white pl-12 pr-10 py-2 rounded-full border border-white/5 focus:outline-none focus:border-[#FF6B00]/50 transition-colors shadow-lg text-sm font-normal"
              />
              {globalSearchQuery.length > 0 && (
                <button
                  type="button"
                  onClick={() => {
                    setGlobalSearchQuery("");
                    const input = document.getElementById('global-search-input') as HTMLInputElement;
                    if (input) input.focus();
                  }}
                  className="absolute right-4 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white transition-colors"
                >
                  <Icon icon={XCircle} className="w-4 h-4" />
                </button>
              )}
            </div>
          </form>

          <div className="flex items-center mr-8 gap-4 z-10">
            <button
              onClick={() => handleNav("settings")}
              className="p-2 text-white/70 hover:text-white bg-white/5 hover:bg-white/10 rounded-full transition-colors focus:outline-none"
            >
              <Icon icon={Settings} className="w-5 h-5" size={20} />
            </button>
          </div>
        </header>

        <div
          className="flex-1 overflow-y-auto relative"
          onScroll={(e) => {
            setIsHeaderScrolled(e.currentTarget.scrollTop > 10 || window.scrollY > 10);
          }}
        >

          {/* Mobile search bar if needed, shown conditionally or stacked */}
          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (currentView !== "search") {
                handleNav("search");
              }
            }}
            className="md:hidden p-4 mt-16"
          >
             <div className="relative">
              <Icon icon={Search} className="absolute left-4 top-1/2 -translate-y-1/2 text-gray-500 w-4 h-4" />
              <input
                type="text"
                placeholder="Quick Search..."
                value={globalSearchQuery}
                onChange={(e) => setGlobalSearchQuery(e.target.value)}
                id="global-search-input-mobile"
                className="w-full bg-[#1F222A]/80 backdrop-blur-xl text-white pl-12 pr-10 py-2 rounded-full border border-white/5 focus:outline-none focus:border-[#FF6B00]/50 transition-colors shadow-lg text-sm font-normal"
              />
              {globalSearchQuery.length > 0 && (
                <button
                  type="button"
                  onClick={() => {
                    setGlobalSearchQuery("");
                    const input = document.getElementById('global-search-input-mobile') as HTMLInputElement;
                    if (input) input.focus();
                  }}
                  className="absolute right-4 top-1/2 -translate-y-1/2 text-gray-400 hover:text-white transition-colors"
                >
                  <Icon icon={XCircle} className="w-4 h-4" />
                </button>
              )}
            </div>
          </form>

          <AnimatePresence mode="wait">
            {selectedMediaId ? (
              <motion.div
                key="details"
                initial={{ opacity: 0, scale: 0.98 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.98 }}
                transition={{ duration: 0.3 }}
                className="h-full w-full pt-16 bg-cinema-black"
                style={{ position: "absolute", zIndex: 20 }}
              >
                <MediaDetails
                  mediaId={selectedMediaId}
                  initialSeasonNum={selectedSeasonNum}
                  onBack={() => handleNav(currentView)}
                  refreshTrigger={refreshTrigger}
                />
              </motion.div>
            ) : (
              <motion.div
                key={currentView}
                initial={{ opacity: 0, scale: currentView === "history" ? 0.98 : 1, y: currentView === "history" ? 0 : 10 }}
                animate={{ opacity: 1, scale: 1, y: 0 }}
                exit={{ opacity: 0, scale: currentView === "history" ? 0.98 : 1, y: currentView === "history" ? 0 : -10 }}
                transition={{ duration: 0.3 }}
                className="h-full w-full pt-16 md:pt-0"
              >
                {currentView === "dashboard" && <Dashboard onMediaSelect={(id: number) => handleNav("dashboard", id)} refreshTrigger={refreshTrigger} searchQuery={globalSearchQuery} />}
                {currentView === "tv" && <Library type="TV" onMediaSelect={(id: number) => handleNav("tv", id)} refreshTrigger={refreshTrigger} searchQuery={globalSearchQuery} />}
                {currentView === "movies" && <Library type="Movie" onMediaSelect={(id: number) => handleNav("movies", id)} refreshTrigger={refreshTrigger} searchQuery={globalSearchQuery} />}
                {currentView === "search" && <SearchTMDB initialQuery={globalSearchQuery} onMediaSelect={(id: number) => handleNav("search", id)} />}
                {currentView === "inbox" && <InboxView onMatch={() => setRefreshTrigger(prev => prev + 1)} />}
                {currentView === "history" && <History onNavigateToMedia={(id: number, seasonNum?: number) => handleNav("history", id, seasonNum)} />}
                {currentView === "settings" && <SettingsView setIsDirty={setIsSettingsDirty} setSaveCallback={setSaveSettingsCallback} />}
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </main>

      {/* Navigation Guard Modal */}
      <AnimatePresence>
          {pendingNavigation && (
              <motion.div
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1 }}
                  exit={{ opacity: 0 }}
                  className="fixed inset-0 z-[100] flex items-center justify-center bg-black/80 backdrop-blur-sm"
              >
                  <motion.div
                      initial={{ scale: 0.95, opacity: 0 }}
                      animate={{ scale: 1, opacity: 1 }}
                      exit={{ scale: 0.95, opacity: 0 }}
                      className="bg-[#1F222A] p-8 rounded-2xl border border-white/10 max-w-md w-full shadow-2xl"
                  >
                      <h2 className="text-2xl font-bold text-white mb-2">Unsaved Changes</h2>
                      <p className="text-muted mb-8">You have unsaved changes in your settings. Do you want to save them before leaving?</p>
                      <div className="flex flex-col gap-3">
                          <button
                              onClick={() => confirmNavigation(true)}
                              className="w-full py-3 bg-[#FF6B00] hover:bg-[#E66000] text-white font-bold rounded-xl transition-colors"
                          >
                              Save & Leave
                          </button>
                          <button
                              onClick={() => confirmNavigation(false)}
                              className="w-full py-3 bg-white/10 hover:bg-white/20 text-white font-bold rounded-xl transition-colors"
                          >
                              Discard & Leave
                          </button>
                          <button
                              onClick={cancelNavigation}
                              className="w-full py-3 bg-transparent hover:bg-white/5 text-muted hover:text-white font-bold rounded-xl transition-colors mt-2"
                          >
                              Cancel
                          </button>
                      </div>
                  </motion.div>
              </motion.div>
          )}
      </AnimatePresence>
      <Toaster
        theme="dark"
        position="bottom-right"
        richColors
        closeButton
        toastOptions={{
          style: {
            background: "#1F222A",
            border: "1px solid rgba(255, 255, 255, 0.1)",
            color: "#FFFFFF",
          },
          className: "border border-white/10 shadow-2xl",
        }}
      />
    </div>
    </MotionConfig>
  );
}

export default App;
