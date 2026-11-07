/** Mirrors the payload produced by `fetch_history` in src-tauri/src/commands.rs. */

export type HistoryFilter = "All" | "TV" | "Movie";

export type WatchState = "completed" | "partial" | "marked" | "archived";

export interface TimeCapsule {
  years: number;
  months: number;
  days: number;
  total_days: number;
  is_early: boolean;
}

export interface HistoryEntry {
  hist_id: number;
  /** Every history row merged into this entry (repeat plays of one episode). */
  hist_ids: number[];
  play_count: number;
  timestamp: number;
  date_key: string;
  formatted_time: string;
  is_utc_fallback: boolean;
  is_legacy: boolean;
  watch_state: WatchState;
  progress_percent: number;
  pause_count: number;
  /** Current resume position of the episode in seconds (not of this play). */
  resume_position: number;
  episode_id: number;
  season_num: number;
  ep_num: number;
  ep_title: string;
  still_path: string;
  air_date: string;
  is_date_known: boolean;
  is_exact_date: boolean;
  time_capsule: TimeCapsule | null;
  runtime: number;
  media_id: number;
  show_title: string;
  media_type: string;
  file_path: string | null;
  poster_path: string;
  backdrop_path: string;
}

export interface HistoryGroup {
  id: string;
  kind: "single" | "session";
  media_id: number;
  media_type: string;
  show_title: string;
  is_legacy: boolean;
  date_key: string;
  start_ts: number;
  end_ts: number;
  watched_minutes: number;
  episode_count: number;
  hist_ids: number[];
  entries: HistoryEntry[];
}

export interface HistoryCursor {
  timestamp: number;
  id: number;
}

export interface HistoryPage {
  groups: HistoryGroup[];
  next_cursor: HistoryCursor | null;
  has_more: boolean;
}

/** View state kept in the app store so returning to the tab is instant. */
export interface HistorySnapshot {
  filter: HistoryFilter;
  groups: HistoryGroup[];
  cursor: HistoryCursor | null;
  hasMore: boolean;
  scrollTop: number;
}
