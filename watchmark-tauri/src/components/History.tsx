import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { AlertTriangle, ArrowUp, Film, History as HistoryIcon, Loader2, RefreshCw, Trash2, Tv } from "lucide-react";
import { useAsyncInvoke } from "../hooks/useAsyncInvoke";
import { useAppStore } from "../store/useAppStore";
import { useUiStore } from "../store/uiStore";
import { logger } from "../utils/logger";
import { toast } from "../utils/toast";
import { HistoryEntryRow } from "./history/HistoryEntryRow";
import { HistorySessionCard } from "./history/HistorySessionCard";
import { formatDayHeading, formatDuration } from "./history/historyFormat";
import type {
  HistoryCursor,
  HistoryEntry,
  HistoryFilter,
  HistoryGroup,
  HistoryPage,
} from "./history/historyTypes";

const PAGE_SIZE = 40;
const UNDO_WINDOW_MS = 5000;
const AUTO_APPLY_SCROLL_THRESHOLD = 160;

const FILTERS: { value: HistoryFilter; label: string; icon: typeof Tv | null }[] = [
  { value: "All", label: "All", icon: null },
  { value: "TV", label: "TV Shows", icon: Tv },
  { value: "Movie", label: "Movies", icon: Film },
];

// ---------------------------------------------------------------------------
// Pending removals live outside the component so the undo window survives
// navigating away from the tab.
// ---------------------------------------------------------------------------
const hiddenHistIds = new Set<number>();
const hiddenListeners = new Set<() => void>();
const notifyHidden = () => hiddenListeners.forEach(fn => fn());

function scheduleRemoval(ids: number[], label: string) {
  const unique = Array.from(new Set(ids));
  unique.forEach(id => hiddenHistIds.add(id));
  notifyHidden();

  let undone = false;
  const restore = () => {
    unique.forEach(id => hiddenHistIds.delete(id));
    notifyHidden();
  };

  const timer = window.setTimeout(async () => {
    if (undone) return;
    try {
      await invoke("delete_history_entries", { historyIds: unique });
      logger.ipcSuccess(`Removed ${unique.length} history entr${unique.length === 1 ? "y" : "ies"}.`);
    } catch (e: any) {
      logger.error("Failed to remove history entries", e);
      restore();
      toast.error(`Couldn't remove "${label}" from history. ${typeof e === "string" ? e : ""}`.trim());
    }
  }, UNDO_WINDOW_MS);

  toast.info(`Removed "${label}" from history`, {
    duration: UNDO_WINDOW_MS,
    action: {
      label: "Undo",
      onClick: () => {
        undone = true;
        window.clearTimeout(timer);
        restore();
        logger.click(`Undo history removal for '${label}'`);
      },
    },
  });
}

function useHiddenIds(): Set<number> {
  const [, force] = useState(0);
  useEffect(() => {
    const fn = () => force(n => n + 1);
    hiddenListeners.add(fn);
    return () => {
      hiddenListeners.delete(fn);
    };
  }, []);
  return hiddenHistIds;
}

const isHistoryPage = (v: unknown): v is HistoryPage =>
  !!v && typeof v === "object" && Array.isArray((v as HistoryPage).groups);

const pageSignature = (groups: HistoryGroup[], count: number) =>
  groups.slice(0, count).map(g => `${g.id}:${g.hist_ids.length}`).join("|");

const describeError = (e: unknown) =>
  typeof e === "string" ? e : e && typeof e === "object" && "message" in e ? String((e as any).message) : "Unknown error";

function friendlyPlayError(e: unknown): string {
  const raw = describeError(e);
  if (raw.includes("FILE_NOT_FOUND")) return "The video file is missing. Rescan your library or relink it from the show page.";
  if (raw.includes("System Busy")) return "WatchMark is optimizing the database. Try again in a moment.";
  return `Couldn't start VLC: ${raw}`;
}

// ---------------------------------------------------------------------------

export default function History({ onNavigateToMedia }: { onNavigateToMedia: (mediaId: number, seasonNum?: number) => void }) {
  const asyncInvoke = useAsyncInvoke();
  const historyVersion = useAppStore(s => s.historyVersion);
  const setHistorySnapshot = useAppStore(s => s.setHistorySnapshot);
  const hiddenIds = useHiddenIds();

  const [filter, setFilter] = useState<HistoryFilter>("All");
  const [groups, setGroups] = useState<HistoryGroup[]>([]);
  const [cursor, setCursor] = useState<HistoryCursor | null>(null);
  const [hasMore, setHasMore] = useState(false);
  const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");
  const [errorText, setErrorText] = useState("");
  const [loadingMore, setLoadingMore] = useState(false);
  const [loadMoreFailed, setLoadMoreFailed] = useState(false);
  const [newActivity, setNewActivity] = useState(false);
  const [launchingId, setLaunchingId] = useState<number | null>(null);
  const [isClearing, setIsClearing] = useState(false);

  const containerRef = useRef<HTMLDivElement>(null);
  const sentinelRef = useRef<HTMLDivElement>(null);
  const requestSeq = useRef(0);
  const loadMoreInFlight = useRef(false);
  const latest = useRef({ filter, groups, cursor, hasMore, status });
  latest.current = { filter, groups, cursor, hasMore, status };

  const fetchPage = useCallback(
    async (after: HistoryCursor | null, f: HistoryFilter): Promise<HistoryPage | null> => {
      const res = await asyncInvoke<HistoryPage>(
        "fetch_history",
        { cursor: after, pageSize: PAGE_SIZE, mediaType: f === "All" ? null : f },
        20000,
      );
      if (res === null) return null; // component unmounted
      if (!isHistoryPage(res)) throw new Error("Unexpected response from fetch_history");
      return res;
    },
    [asyncInvoke],
  );

  const applyFirstPage = useCallback((page: HistoryPage) => {
    setGroups(page.groups);
    setCursor(page.next_cursor);
    setHasMore(page.has_more);
    setLoadMoreFailed(false);
    setNewActivity(false);
    setStatus("ready");
  }, []);

  /** Loads the newest page, replacing the list. */
  const loadFirst = useCallback(
    async (f: HistoryFilter) => {
      const seq = ++requestSeq.current;
      loadMoreInFlight.current = false;
      setStatus("loading");
      setGroups([]);
      setLoadingMore(false);
      try {
        const page = await fetchPage(null, f);
        if (!page || seq !== requestSeq.current) return;
        applyFirstPage(page);
        containerRef.current?.scrollTo({ top: 0 });
      } catch (e) {
        if (seq !== requestSeq.current) return;
        logger.error("Failed to load watch history", e);
        setErrorText(describeError(e));
        setStatus("error");
      }
    },
    [fetchPage, applyFirstPage],
  );

  /**
   * Re-fetches the newest page in the background. Applies it straight away when
   * the user is at the top, otherwise offers a "new activity" pill.
   */
  const revalidate = useCallback(async () => {
    const seq = requestSeq.current;
    const f = latest.current.filter;
    try {
      const page = await fetchPage(null, f);
      if (!page || seq !== requestSeq.current || f !== latest.current.filter) return;
      const current = latest.current.groups;
      if (pageSignature(page.groups, page.groups.length) === pageSignature(current, page.groups.length)) return;
      const atTop = (containerRef.current?.scrollTop ?? 0) < AUTO_APPLY_SCROLL_THRESHOLD;
      if (atTop || latest.current.status !== "ready" || current.length === 0) {
        requestSeq.current++;
        applyFirstPage(page);
      } else {
        setNewActivity(true);
      }
    } catch (e) {
      logger.error("Background history refresh failed", e);
    }
  }, [fetchPage, applyFirstPage]);

  const loadMore = useCallback(async () => {
    const { hasMore: more, cursor: after, status: st, filter: f } = latest.current;
    if (loadMoreInFlight.current || !more || !after || st !== "ready") return;
    loadMoreInFlight.current = true;
    const seq = requestSeq.current;
    setLoadingMore(true);
    setLoadMoreFailed(false);
    try {
      const page = await fetchPage(after, f);
      if (!page || seq !== requestSeq.current) return;
      setGroups(prev => {
        const seen = new Set(prev.map(g => g.id));
        return [...prev, ...page.groups.filter(g => !seen.has(g.id))];
      });
      setCursor(page.next_cursor);
      setHasMore(page.has_more);
    } catch (e) {
      if (seq !== requestSeq.current) return;
      logger.error("Failed to load older history", e);
      setLoadMoreFailed(true);
    } finally {
      if (seq === requestSeq.current) {
        loadMoreInFlight.current = false;
        setLoadingMore(false);
      }
    }
  }, [fetchPage]);

  // Mount: restore the previous view instantly, then make sure it's current.
  useEffect(() => {
    const { historySnapshot, historyPrefetch, historyVersion: version } = useAppStore.getState();
    if (historySnapshot && historySnapshot.version === version) {
      setFilter(historySnapshot.filter);
      latest.current.filter = historySnapshot.filter;
      setGroups(historySnapshot.groups);
      latest.current.groups = historySnapshot.groups;
      setCursor(historySnapshot.cursor);
      setHasMore(historySnapshot.hasMore);
      setStatus("ready");
      latest.current.status = "ready";
      requestAnimationFrame(() => {
        if (containerRef.current) containerRef.current.scrollTop = historySnapshot.scrollTop;
        revalidate();
      });
    } else if (historyPrefetch && historyPrefetch.version === version) {
      applyFirstPage(historyPrefetch.page);
      latest.current.groups = historyPrefetch.page.groups;
      latest.current.status = "ready";
      revalidate();
    } else {
      loadFirst("All");
    }

    return () => {
      const s = latest.current;
      if (s.status === "ready") {
        setHistorySnapshot({
          filter: s.filter,
          groups: s.groups,
          cursor: s.cursor,
          hasMore: s.hasMore,
          scrollTop: containerRef.current?.scrollTop ?? 0,
        });
      } else {
        setHistorySnapshot(null);
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // New plays / imports recorded elsewhere in the app.
  const mountedVersion = useRef(historyVersion);
  useEffect(() => {
    if (historyVersion === mountedVersion.current) return;
    mountedVersion.current = historyVersion;
    revalidate();
  }, [historyVersion, revalidate]);

  // Drop a deleted show's entries immediately, wherever the user is scrolled.
  useEffect(() => {
    const unlisten = listen<{ media_id: number }>("media-deleted", event => {
      const id = event.payload?.media_id;
      if (typeof id === "number") setGroups(prev => prev.filter(g => g.media_id !== id));
    });
    return () => {
      unlisten.then(fn => fn()).catch(() => {});
    };
  }, []);

  // Infinite scroll.
  useEffect(() => {
    const root = containerRef.current;
    const target = sentinelRef.current;
    if (!root || !target || !hasMore || status !== "ready" || loadMoreFailed) return;
    const observer = new IntersectionObserver(
      entries => {
        if (entries[0]?.isIntersecting) loadMore();
      },
      { root, rootMargin: "800px 0px" },
    );
    observer.observe(target);
    return () => observer.disconnect();
  }, [hasMore, status, loadMoreFailed, loadMore, groups.length]);

  // ------------------------------------------------------------------ actions

  const changeFilter = (f: HistoryFilter) => {
    if (f === filter) return;
    logger.click(`History filter '${f}'`);
    setFilter(f);
    latest.current.filter = f;
    loadFirst(f);
  };

  const showLatest = () => {
    logger.click("Show latest history activity");
    loadFirst(filter);
  };

  const openEntry = useCallback(
    (entry: HistoryEntry) => onNavigateToMedia(entry.media_id, entry.season_num),
    [onNavigateToMedia],
  );

  const launchingRef = useRef<number | null>(null);
  const playEntry = useCallback(async (entry: HistoryEntry) => {
    if (!entry.file_path || launchingRef.current !== null) return;
    launchingRef.current = entry.episode_id;
    setLaunchingId(entry.episode_id);
    logger.click(`Play from History: '${entry.show_title}' S${entry.season_num}E${entry.ep_num}`);
    try {
      await invoke("play_episode_cmd", {
        episodeId: entry.episode_id,
        filePath: entry.file_path,
        lastPosition: entry.watch_state === "partial" ? entry.resume_position : 0,
      });
      logger.ipcSuccess("VLC playback launched from History");
    } catch (e) {
      logger.error("VLC launch failed from History", e);
      toast.error(friendlyPlayError(e));
    } finally {
      launchingRef.current = null;
      setLaunchingId(null);
    }
  }, []);

  const removeEntry = useCallback((entry: HistoryEntry) => {
    const label = entry.media_type === "Movie" ? entry.show_title : `${entry.show_title} S${entry.season_num}E${entry.ep_num}`;
    logger.click(`Remove '${label}' from history`);
    scheduleRemoval(entry.hist_ids, label);
  }, []);

  const removeGroup = useCallback((group: HistoryGroup) => {
    const label = `${group.show_title} (${group.episode_count} episodes)`;
    logger.click(`Remove session '${label}' from history`);
    scheduleRemoval(group.hist_ids, label);
  }, []);

  const clearAll = async () => {
    const ok = await useUiStore
      .getState()
      .showConfirm(
        "Clear watch history?",
        "This permanently removes every entry from your watch history. Watched status and resume points on your shows are not affected. This can't be undone.",
      );
    if (!ok) return;
    logger.click("Clear entire watch history");
    setIsClearing(true);
    try {
      const removed = await invoke<number>("clear_history");
      requestSeq.current++;
      hiddenHistIds.clear();
      notifyHidden();
      setGroups([]);
      setCursor(null);
      setHasMore(false);
      setNewActivity(false);
      setStatus("ready");
      toast.success(`Watch history cleared (${removed} entr${removed === 1 ? "y" : "ies"}).`);
    } catch (e) {
      logger.error("Failed to clear history", e);
      toast.error(`Couldn't clear history: ${describeError(e)}`);
    } finally {
      setIsClearing(false);
    }
  };

  // ------------------------------------------------------------------ derived

  const sections = useMemo(() => {
    const out: { key: string; groups: HistoryGroup[]; minutes: number }[] = [];
    for (const group of groups) {
      const entries = group.entries.filter(e => !e.hist_ids.every(id => hiddenIds.has(id)));
      if (entries.length === 0) continue;
      const visible: HistoryGroup =
        entries.length === group.entries.length
          ? group
          : {
              ...group,
              kind: entries.length > 1 ? "session" : "single",
              entries,
              episode_count: entries.length,
              hist_ids: group.hist_ids.filter(id => !hiddenIds.has(id)),
            };
      const last = out[out.length - 1];
      if (last && last.key === visible.date_key) {
        last.groups.push(visible);
        last.minutes += visible.watched_minutes;
      } else {
        out.push({ key: visible.date_key, groups: [visible], minutes: visible.watched_minutes });
      }
    }
    return out;
    // hiddenIds is a stable Set mutated in place; useHiddenIds re-renders on change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [groups, hiddenIds, hiddenIds.size]);

  const isEmpty = status === "ready" && sections.length === 0 && !hasMore;

  // ------------------------------------------------------------------ render

  return (
    <div id="turbo-scroll-history" ref={containerRef} className="h-full overflow-y-auto px-6 sm:px-12 pt-24 pb-24">
      <div className="max-w-4xl mx-auto">
        <header className="flex flex-col sm:flex-row sm:items-end justify-between gap-4 mb-8">
          <div>
            <h1 className="text-4xl font-extrabold tracking-tight">Watch History</h1>
            <p className="text-[#8E929C] mt-1">Everything you've watched, newest first.</p>
          </div>
          <div className="flex items-center gap-3">
            <div role="group" aria-label="Filter history" className="flex bg-[#1F222A] border border-[#2A2D35] rounded-full p-1">
              {FILTERS.map(({ value, label, icon: IconCmp }) => (
                <button
                  key={value}
                  type="button"
                  aria-pressed={filter === value}
                  onClick={() => changeFilter(value)}
                  className={`flex items-center gap-1.5 px-4 py-1.5 rounded-full text-sm font-bold transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[#FF6B00] ${
                    filter === value ? "bg-[#FF6B00] text-white" : "text-[#A0AEC0] hover:text-white"
                  }`}
                >
                  {IconCmp && <IconCmp size={14} aria-hidden="true" />}
                  {label}
                </button>
              ))}
            </div>
            <button
              type="button"
              onClick={clearAll}
              disabled={isClearing || groups.length === 0}
              className="flex items-center gap-1.5 px-3 py-2 rounded-full text-sm font-bold text-[#A0AEC0] hover:text-red-400 hover:bg-red-500/10 transition-colors disabled:opacity-40 disabled:pointer-events-none focus:outline-none focus-visible:ring-2 focus-visible:ring-[#FF6B00]"
              title="Clear watch history"
            >
              {isClearing ? <Loader2 size={14} className="animate-spin" aria-hidden="true" /> : <Trash2 size={14} aria-hidden="true" />}
              <span className="hidden md:inline">Clear history</span>
            </button>
          </div>
        </header>

        {newActivity && (
          <div className="sticky top-[72px] z-40 flex justify-center pointer-events-none mb-4">
            <button
              type="button"
              onClick={showLatest}
              className="pointer-events-auto flex items-center gap-2 px-4 py-2 rounded-full bg-[#FF6B00] text-white text-sm font-bold shadow-[0_4px_14px_rgba(255,107,0,0.4)] hover:bg-[#FF8533] transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-white"
            >
              <ArrowUp size={14} aria-hidden="true" /> New activity. Show latest
            </button>
          </div>
        )}

        {status === "loading" && <HistorySkeleton />}

        {status === "error" && (
          <div role="alert" className="flex flex-col items-center justify-center text-center py-24">
            <AlertTriangle size={56} className="text-[#FF6B00]/70 mb-5" aria-hidden="true" />
            <h2 className="text-2xl font-bold text-white mb-2">Couldn't load your watch history</h2>
            <p className="text-[#8E929C] max-w-md mb-1">Your history is safe. Something went wrong while reading it.</p>
            <p className="text-xs text-white/30 max-w-md mb-6 break-words">{errorText}</p>
            <button
              type="button"
              onClick={() => loadFirst(filter)}
              className="flex items-center gap-2 px-5 py-2.5 rounded-full bg-[#FF6B00] hover:bg-[#FF8533] text-white font-bold transition-colors"
            >
              <RefreshCw size={16} aria-hidden="true" /> Try again
            </button>
          </div>
        )}

        {isEmpty && (
          <div className="flex flex-col items-center justify-center text-center py-24">
            <HistoryIcon size={110} strokeWidth={1.25} className="text-white/5 mb-6" aria-hidden="true" />
            <h2 className="text-3xl font-bold text-white mb-2">
              {filter === "All" ? "Your watch history is empty" : filter === "TV" ? "No TV episodes in your history" : "No movies in your history"}
            </h2>
            <p className="text-[#A0AEC0] max-w-md">
              {filter === "All"
                ? "Episodes and movies you play in VLC, or mark as watched, will show up here."
                : "Try another filter to see the rest of your history."}
            </p>
            {filter !== "All" && (
              <button
                type="button"
                onClick={() => changeFilter("All")}
                className="mt-6 px-5 py-2.5 rounded-full bg-[#FF6B00] hover:bg-[#FF8533] text-white font-bold transition-colors"
              >
                Show all history
              </button>
            )}
          </div>
        )}

        {status === "ready" &&
          sections.map(section => (
            <section key={section.key} className="mb-10" aria-label={formatDayHeading(section.key)}>
              <div className="sticky top-[64px] z-30 -mx-2 px-2 py-2 mb-3 bg-[#0D0F14]/90 backdrop-blur-md flex items-baseline justify-between">
                <h2 className="text-lg font-bold text-white">{formatDayHeading(section.key)}</h2>
                <span className="text-xs text-[#8E929C] tabular-nums">
                  {section.groups.length} {section.groups.length === 1 ? "title" : "titles"} · {formatDuration(section.minutes)}
                </span>
              </div>
              <ul className="flex flex-col gap-3">
                {section.groups.map(group => (
                  <li key={group.id}>
                    {group.kind === "session" ? (
                      <HistorySessionCard
                        group={group}
                        launchingId={launchingId}
                        onOpen={openEntry}
                        onPlay={playEntry}
                        onRemoveEntry={removeEntry}
                        onRemoveGroup={removeGroup}
                      />
                    ) : (
                      <HistoryEntryRow
                        entry={group.entries[0]}
                        variant="card"
                        isLaunching={launchingId === group.entries[0].episode_id}
                        onOpen={openEntry}
                        onPlay={playEntry}
                        onRemove={removeEntry}
                      />
                    )}
                  </li>
                ))}
              </ul>
            </section>
          ))}

        {status === "ready" && hasMore && (
          <div ref={sentinelRef} className="flex justify-center items-center h-20" aria-live="polite">
            {loadMoreFailed ? (
              <button
                type="button"
                onClick={loadMore}
                className="flex items-center gap-2 px-4 py-2 rounded-full bg-white/10 hover:bg-white/20 text-white text-sm font-bold transition-colors"
              >
                <RefreshCw size={14} aria-hidden="true" /> Couldn't load older history. Retry
              </button>
            ) : (
              <Loader2 size={24} className={`text-[#FF6B00] ${loadingMore ? "animate-spin" : "opacity-0"}`} aria-label="Loading older history" />
            )}
          </div>
        )}

        {status === "ready" && !hasMore && sections.length > 0 && (
          <p className="text-center text-xs text-white/25 py-8">You've reached the beginning of your history.</p>
        )}
      </div>
    </div>
  );
}

function HistorySkeleton() {
  return (
    <div aria-busy="true" aria-label="Loading watch history">
      {[3, 2].map((rows, s) => (
        <div key={s} className="mb-10">
          <div className="h-6 w-32 rounded-md skeleton-loader mb-4" />
          <div className="flex flex-col gap-3">
            {Array.from({ length: rows }).map((_, i) => (
              <div key={i} className="flex items-center gap-4 p-3 rounded-xl bg-[#1F222A]/60 border border-[#2A2D35]">
                <div className="w-36 sm:w-44 aspect-video rounded-lg skeleton-loader" />
                <div className="flex-1 space-y-2">
                  <div className="h-4 w-1/2 rounded skeleton-loader" />
                  <div className="h-3 w-1/3 rounded skeleton-loader" />
                  <div className="h-3 w-1/4 rounded skeleton-loader" />
                </div>
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  );
}
