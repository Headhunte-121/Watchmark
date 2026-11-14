import { formatImagePath } from "../utils/imageFormat";
import { useState, useEffect, useRef, useCallback } from "react";
import { Search, Tv, Clapperboard, XCircle } from "lucide-react";
import { formatLocaleDate } from "../utils/dateFormatter";
import { useAsyncInvoke } from "../hooks/useAsyncInvoke";
import { logger } from "../utils/logger";
import { motion } from "framer-motion";
import { toast } from "../utils/toast";
import { tmdbUnavailableReason } from "../utils/connectivity";
import { useAppStore } from "../store/useAppStore";
import { cn } from "../App";
import { SafeImage } from "./ui/SafeImage";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";

function useDebounce<T>(value: T, delay: number): T {
  const [debouncedValue, setDebouncedValue] = useState<T>(value);

  useEffect(() => {
    const handler = setTimeout(() => {
      setDebouncedValue(value);
    }, delay);

    return () => {
      clearTimeout(handler);
    };
  }, [value, delay]);

  return debouncedValue;
}

export default function SearchTMDB({ initialQuery, onMediaSelect: _onMediaSelect }: any) {
  const [query, setQuery] = useState(initialQuery || "");
  const debouncedQuery = useDebounce(query, 500);
  const [results, setResults] = useState<any[]>([]);
  const [loading, setLoading] = useState(false);
  const [_page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(true);
  const asyncInvoke = useAsyncInvoke();
  const { isApiAuthorized, isOffline, connectivity } = useAppStore();
  const observer = useRef<IntersectionObserver | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [filterType, setFilterType] = useState<"All" | "TV" | "Movies">("All");
  const [trackedIds, setTrackedIds] = useState<Set<string>>(new Set());
  const [addingIds, setAddingIds] = useState<Set<string>>(new Set());

  // Fetch local media cache on mount
  useEffect(() => {
    async function fetchTrackedIds() {
        try {
            const data: any[] = await invoke("get_library_data_db", { sortBy: "Recently Added", filter: "All" });
            const ids = new Set(data.map(m => `${m.type}-${m.tmdb_id}`));
            setTrackedIds(ids);
        } catch (e) {
            console.error("Failed to fetch library for tracked IDs", e);
        }
    }
    fetchTrackedIds();

    // Subscribe to MediaDeleted event to update tracked IDs real-time
    const unlisten = listen("media-deleted", (_event: any) => {
        fetchTrackedIds();
    });

    return () => {
        unlisten.then(f => f());
    };
  }, []);

  useEffect(() => {
    // Initial page-load auto-focus
    const timer = setTimeout(() => {
      if (inputRef.current) {
        inputRef.current.focus();
      }
    }, 100);
    return () => clearTimeout(timer);
  }, []);

  const performSearch = useCallback(async (searchQuery: string, pageNum: number, append: boolean = false) => {
    if (!searchQuery.trim()) {
        setResults([]);
        return;
    }

    if (pageNum === 1) {
        logger.input(`Search Bar for '${searchQuery}'`);
    } else {
        logger.action(`Scrolling for more results (Page ${pageNum})`);
    }

    setLoading(true);
    try {
      const res: any = await asyncInvoke("perform_tmdb_search", { query: searchQuery, page: pageNum });
      if (res) {
        if (append) {
            setResults(prev => {
                // Deduplicate strictly by tmdb_id just in case the backend overlaps due to rapid scrolls
                const newIds = new Set(res.map((r: any) => `${r.type}-${r.tmdb_id}`));
                const filteredPrev = prev.filter(r => !newIds.has(`${r.type}-${r.tmdb_id}`));
                return [...filteredPrev, ...res];
            });
        } else {
            setResults(res);
        }
        setHasMore(res.length > 0);
      }
    } catch (e: any) {
      const errStr = typeof e === 'object' && e.message ? e.message : String(e);
      logger.error("TMDB Search Failed", errStr);
      if (errStr.includes("API Key is invalid") || errStr.includes("401") || errStr.includes("Unauthorized") || errStr.includes("API Key was revoked")) {
        toast.error("Invalid API Key: Please check your TMDB API Key in Settings.");
      } else if (errStr.includes("Task Timed Out") || errStr.includes("timeout") || errStr.toLowerCase().includes("connection")) {
        toast.error("TMDB is taking too long to respond. Please check your internet connection.", { duration: 6000 });
      } else {
        // Fallback for other errors (they might be handled silently or show a generic toast)
        toast.error("Search failed: " + errStr);
      }
    } finally {
      setLoading(false);
    }
  }, [asyncInvoke]);

  // Sync if initialQuery changes from props, e.g. typing in global search bar then navigating
  useEffect(() => {
    if (initialQuery !== undefined && initialQuery !== query) {
      setQuery(initialQuery);
    }
  }, [initialQuery]);

  useEffect(() => {
    setPage(1);
    setHasMore(true);
    performSearch(debouncedQuery, 1, false);
  }, [debouncedQuery, performSearch]);

  const lastElementRef = useCallback((node: HTMLDivElement) => {
    if (loading) return;
    if (observer.current) observer.current.disconnect();

    observer.current = new IntersectionObserver(entries => {
        if (entries[0].isIntersecting && hasMore) {
            setPage(prevPage => {
                const nextPage = prevPage + 1;
                performSearch(debouncedQuery, nextPage, true);
                return nextPage;
            });
        }
    });

    if (node) observer.current.observe(node);
  }, [loading, hasMore, debouncedQuery, performSearch]);

  const filteredResults = results.filter((item) => {
    if (filterType === "All") return true;
    if (filterType === "TV") return item.type === "TV";
    if (filterType === "Movies") return item.type === "Movie";
    return true;
  });

  return (
    <div className="max-w-[1800px] mx-auto px-6 py-24 pb-24 relative w-full h-full">
      <div className="flex items-center gap-4 mb-8 justify-between">
        <h1 className="text-4xl font-extrabold tracking-tight">Discover Media</h1>
        <div className="flex items-center bg-[#1F222A] p-1 rounded-full border border-white/5 shadow-inner">
          {["All", "TV", "Movies"].map((type) => (
            <button
              key={type}
              onClick={() => setFilterType(type as any)}
              className={cn(
                "px-6 py-2 rounded-full text-sm font-bold transition-all duration-200",
                filterType === type ? "bg-[#FF6B00] text-white shadow-md" : "text-muted hover:text-white"
              )}
            >
              {type}
            </button>
          ))}
        </div>
      </div>

      {!isApiAuthorized && (
          <div className="mb-12 max-w-2xl w-full bg-[#FF6B00]/10 border border-[#FF6B00]/20 rounded-xl p-8 text-center">
              <h2 className="text-xl font-bold text-white mb-4">TMDB API Key Required</h2>
              <p className="text-gray-300 mb-6 max-w-lg mx-auto">
                  WatchMark uses the free TMDB service to provide high-quality posters, backdrops, and episode details. You need to connect your own free account.
              </p>
              <button
                  onClick={async () => {
                      try {
                          const { open } = await import('@tauri-apps/plugin-shell');
                          await open("https://www.themoviedb.org/settings/api");
                      } catch (e) {
                          console.error("Failed to open URL:", e);
                      }
                  }}
                  className="px-6 py-3 bg-[#FF6B00] hover:bg-[#E66000] text-white font-bold rounded-xl transition-colors"
              >
                  Get your free API key here
              </button>
          </div>
      )}

      <form
        onSubmit={(e) => {
          e.preventDefault();
          setPage(1);
          performSearch(query, 1, false);
        }}
        className="flex gap-4 mb-12 max-w-2xl"
      >
        <div className="relative flex-1">
          <Search className={cn("absolute left-4 top-1/2 -translate-y-1/2 w-5 h-5", isApiAuthorized ? "text-muted" : "text-gray-600")} />
          <input
            ref={inputRef}
            type="text"
            placeholder={isApiAuthorized ? "Search TMDB for Shows or Movies..." : "API Key Required"}
            value={query}
            disabled={!isApiAuthorized}
            title={!isApiAuthorized ? "API Key is required to search. Add it in Settings." : ""}
            onChange={(e) => {
              setQuery(e.target.value);
              if (e.target.value === "") {
                setResults([]);
              }
            }}
            className="w-full bg-[#1F222A] text-white pl-12 pr-12 py-4 rounded-xl border border-white/5 focus:outline-none focus:border-[#FF6B00]/50 transition-colors shadow-inner font-normal text-lg disabled:opacity-50 disabled:cursor-not-allowed relative overflow-hidden"
          />
          {query.length > 0 && (
            <button
              type="button"
              onClick={() => {
                setQuery("");
                setResults([]);
                if (inputRef.current) inputRef.current.focus();
              }}
              className="absolute right-4 top-1/2 -translate-y-1/2 text-gray-500 hover:text-white transition-colors"
            >
              <XCircle className="w-5 h-5" />
            </button>
          )}
          {loading && (
             <div className="absolute top-0 left-0 h-1 bg-[#FF6B00] animate-pulse rounded-t-xl" style={{ width: '100%' }} />
          )}
        </div>
        <button
          type="submit"
          disabled={loading || !isApiAuthorized || isOffline}
          title={isOffline ? tmdbUnavailableReason(connectivity) : ""}
          className={cn(
            "px-8 py-4 font-bold rounded-full transition-all shadow-lg",
            loading || !isApiAuthorized || isOffline
              ? "bg-gray-600 text-gray-300 opacity-40 cursor-not-allowed grayscale pointer-events-none"
              : "bg-[#FF6B00] hover:bg-[#E66000] text-white hover:scale-105 shadow-orange-500/20"
          )}
        >
          {loading ? "Searching..." : "Search TMDB"}
        </button>
      </form>

      <div className={cn(
        "gap-6 md:gap-8 pb-24",
        filteredResults.length <= 2 && filteredResults.length > 0 ? "flex justify-center max-w-3xl mx-auto" : "grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))]"
      )}>
        {filteredResults.map((item, i) => (
          <motion.div
            layout
            key={`${item.type}-${item.tmdb_id}`}
            initial={{ opacity: 0, scale: 0.9 }}
            animate={{ opacity: 1, scale: 1 }}
            whileHover={document.documentElement.getAttribute('data-cinema-mode') !== 'false' ? { scale: 1.05 } : {}}
            transition={document.documentElement.getAttribute('data-cinema-mode') !== 'false' ? { delay: (i % 20) * 0.05, duration: 0.2 } : { duration: 0 }}
            className={cn(
              "relative aspect-[2/3] bg-[#1F222A] rounded-xl overflow-hidden group shadow-md hover:shadow-2xl transition-shadow",
              filteredResults.length <= 2 ? "w-[180px] md:w-[220px] flex-none" : "w-full"
            )}
          >
            {/* Visual Badging */}
            <div className="absolute top-2 left-2 z-10">
              {item.type === "TV" ? (
                  <div className="flex items-center gap-1.5 px-2 py-1 bg-black/80 backdrop-blur-md border border-emerald-500/50 rounded-full shadow-lg">
                      <Tv className="w-3 h-3 text-emerald-400" />
                      <span className="text-[10px] font-bold text-emerald-400 uppercase tracking-widest">TV</span>
                  </div>
              ) : (
                  <div className="flex items-center gap-1.5 px-2 py-1 bg-black/80 backdrop-blur-md border border-blue-500/50 rounded-full shadow-lg">
                      <Clapperboard className="w-3 h-3 text-blue-400" />
                      <span className="text-[10px] font-bold text-blue-400 uppercase tracking-widest">MOVIE</span>
                  </div>
              )}
            </div>

            <SafeImage
              srcPath={item.poster_path ? formatImagePath(item.poster_path, "w500") : ""}
              type="poster"
              title={item.title}
              altText={item.title}
              className="w-full h-full object-cover transition-transform duration-500 group-hover:scale-110"
            />

            <div className="absolute inset-0 bg-black/60 opacity-0 group-hover:opacity-100 transition-opacity duration-300 flex flex-col justify-end p-4 z-20">
              <h3 className="text-white font-bold leading-tight mb-1 line-clamp-2">{item.title}</h3>
              <p className="text-xs text-gray-300 mb-4 uppercase tracking-wider font-bold flex items-center gap-2 tabular-nums">
                 <span>{item.is_date_known ? (item.type === "TV" ? `${item.release_date.substring(0, 4)}–` : (item.is_exact_date ? formatLocaleDate(item.release_date) : item.release_date.substring(0, 4))) : <span className="px-1.5 py-0.5 bg-gray-800 rounded text-xs font-semibold uppercase tracking-wider text-muted">TBD</span>}</span>
              </p>

              {trackedIds.has(`${item.type}-${item.tmdb_id}`) ? (
                <div className="w-full py-2 bg-emerald-500/20 text-emerald-500 font-bold rounded-lg flex items-center justify-center gap-2 pointer-events-none">
                  <span className="text-emerald-500 font-black">✓</span> IN LIBRARY
                </div>
              ) : (
                <button
                  onClick={() => {
                    logger.click(`'+ Add to Tracker' for '${item.title}'`);
                    const itemId = `${item.type}-${item.tmdb_id}`;
                    setAddingIds(prev => new Set(prev).add(itemId));
                    asyncInvoke("add_to_tracker", {
                      tmdbId: item.tmdb_id,
                      mediaType: item.type,
                      archive: false
                    }).then(() => {
                      logger.ipcSuccess(`'${item.title}' added successfully.`);
                      toast.success("Added to Tracker!");
                      setTrackedIds(prev => new Set(prev).add(itemId));
                      setAddingIds(prev => {
                          const next = new Set(prev);
                          next.delete(itemId);
                          return next;
                      });
                    }).catch((e: any) => {
                      const errStr = typeof e === 'object' && e.message ? e.message : String(e);
                      logger.error("Add to Tracker Failed", errStr);
                      toast.error("Error: " + errStr);
                      setAddingIds(prev => {
                          const next = new Set(prev);
                          next.delete(itemId);
                          return next;
                      });
                    });
                  }}
                  disabled={addingIds.has(`${item.type}-${item.tmdb_id}`)}
                  className={cn(
                    "w-full py-2 font-bold rounded-lg transition-colors flex items-center justify-center",
                    addingIds.has(`${item.type}-${item.tmdb_id}`)
                      ? "bg-gray-400 text-white cursor-wait opacity-80 animate-pulse"
                      : "bg-[#FF6B00] text-white hover:bg-[#E66000]"
                  )}
                >
                  {addingIds.has(`${item.type}-${item.tmdb_id}`) ? "Adding..." : "+ Add to Tracker"}
                </button>
              )}
            </div>
          </motion.div>
        ))}

        {/* Intersection Observer target for infinite scrolling */}
        {results.length > 0 && hasMore && (
           <div ref={lastElementRef} className="col-span-full h-10 flex items-center justify-center">
              {loading && <div className="w-6 h-6 border-2 border-[#FF6B00] border-t-transparent rounded-full animate-spin" />}
           </div>
        )}
      </div>
    </div>
  );
}
