import type { HistoryEntry, HistoryGroup, TimeCapsule } from "./historyTypes";

const ordinal = (d: number) => {
  if (d > 3 && d < 21) return "th";
  switch (d % 10) {
    case 1: return "st";
    case 2: return "nd";
    case 3: return "rd";
    default: return "th";
  }
};

/** Parses a backend `YYYY-MM-DD` key as a local calendar date. */
export function parseDateKey(key: string): Date | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(key ?? "");
  if (!m) return null;
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
  return Number.isNaN(d.getTime()) ? null : d;
}

/** "Today", "Yesterday", "Monday" (this week), else "Monday, March 10th[, 2024]". */
export function formatDayHeading(key: string, now: Date = new Date()): string {
  const d = parseDateKey(key);
  if (!d) return "Unknown date";
  const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
  const diffDays = Math.round((today.getTime() - d.getTime()) / 86_400_000);
  if (diffDays === 0) return "Today";
  if (diffDays === 1) return "Yesterday";
  const weekday = d.toLocaleDateString("en-US", { weekday: "long" });
  if (diffDays > 1 && diffDays < 7) return weekday;
  const month = d.toLocaleDateString("en-US", { month: "long" });
  const day = d.getDate();
  const year = d.getFullYear() !== now.getFullYear() ? `, ${d.getFullYear()}` : "";
  return `${weekday}, ${month} ${day}${ordinal(day)}${year}`;
}

export function formatDuration(minutes: number): string {
  const total = Math.max(0, Math.round(minutes || 0));
  if (total < 60) return `${total}m`;
  const h = Math.floor(total / 60);
  const m = total % 60;
  return m === 0 ? `${h}h` : `${h}h ${m}m`;
}

export function formatClock(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds || 0));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`
    : `${m}:${String(sec).padStart(2, "0")}`;
}

export function episodeCode(e: Pick<HistoryEntry, "season_num" | "ep_num">): string {
  return e.season_num === 0 ? `Special ${e.ep_num}` : `S${e.season_num} · E${e.ep_num}`;
}

export function isMovie(mediaType: string): boolean {
  return mediaType === "Movie";
}

const vibe = (date: Date) => {
  const h = date.getHours();
  if (h >= 5 && h < 12) return "Morning";
  if (h >= 12 && h < 17) return "Afternoon";
  if (h >= 17 && h < 21) return "Evening";
  return "Late Night";
};

/** "Fri Evening" or "Fri Evening – Sat Late Night" for sessions crossing midnight. */
export function sessionVibe(group: HistoryGroup): { label: string; crossesMidnight: boolean } {
  const start = new Date(group.start_ts * 1000);
  const end = new Date(group.end_ts * 1000);
  const day = (d: Date) => d.toLocaleDateString("en-US", { weekday: "short" });
  const crossesMidnight = start.toDateString() !== end.toDateString();
  return {
    label: crossesMidnight
      ? `${day(start)} ${vibe(start)} – ${day(end)} ${vibe(end)}`
      : `${day(start)} ${vibe(start)}`,
    crossesMidnight,
  };
}

/** "S1 · E3 – S1 · E7" for a session (entries are newest first). */
export function sessionRange(group: HistoryGroup): string {
  const eps = [...group.entries].sort((a, b) => a.season_num - b.season_num || a.ep_num - b.ep_num);
  const first = eps[0];
  const last = eps[eps.length - 1];
  if (!first) return "";
  return first.episode_id === last.episode_id
    ? episodeCode(first)
    : `${episodeCode(first)} – ${episodeCode(last)}`;
}

export function timeCapsuleLabel(tc: TimeCapsule | null): { text: string; tone: "early" | "premiere" | "muted" } | null {
  if (!tc) return null;
  if (tc.is_early) return { text: "Early watch", tone: "early" };
  if (tc.total_days === 0) return { text: "Watched on premiere day", tone: "premiere" };
  const span =
    tc.years >= 1 ? `${tc.years}y` :
    tc.months >= 1 ? `${tc.months}mo` :
    `${tc.days}d`;
  return { text: `Watched ${span} after airing`, tone: "muted" };
}
