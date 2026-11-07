import { memo } from "react";
import { CheckCircle2, ChevronRight, Loader2, Play, RotateCcw, Trash2 } from "lucide-react";
import { SafeImage } from "../ui/SafeImage";
import { formatImagePath } from "../../utils/imageFormat";
import type { HistoryEntry } from "./historyTypes";
import { episodeCode, formatClock, isMovie, timeCapsuleLabel } from "./historyFormat";

interface HistoryEntryRowProps {
  entry: HistoryEntry;
  /** "card" is a standalone timeline row, "nested" sits inside a session card. */
  variant: "card" | "nested";
  isLaunching: boolean;
  onOpen: (entry: HistoryEntry) => void;
  onPlay: (entry: HistoryEntry) => void;
  onRemove: (entry: HistoryEntry) => void;
}

function StatusLine({ entry }: { entry: HistoryEntry }) {
  switch (entry.watch_state) {
    case "completed":
      return (
        <span className="flex items-center gap-1 text-emerald-400">
          <CheckCircle2 size={12} aria-hidden="true" /> Watched
        </span>
      );
    case "marked":
      return <span className="text-[#A0AEC0]">Marked as watched</span>;
    case "archived":
      return (
        <span className="bg-white/10 text-[#A0AEC0] font-bold text-[10px] px-2 py-0.5 rounded-full uppercase tracking-wider">
          Archived
        </span>
      );
    default: {
      // Position this play stopped at, derived from its own completion ratio.
      const stoppedAt = Math.round((entry.progress_percent / 100) * entry.runtime * 60);
      return (
        <span className="flex items-center gap-2 min-w-0">
          {stoppedAt > 0 && <span className="text-[#FF6B00] tabular-nums">Paused at {formatClock(stoppedAt)}</span>}
          <span className="w-20 h-1 rounded-full bg-white/10 overflow-hidden flex-shrink-0" aria-hidden="true">
            <span
              className="block h-full bg-[#FF6B00] rounded-full"
              style={{ width: `${Math.min(100, Math.max(2, entry.progress_percent))}%` }}
            />
          </span>
          <span className="text-[#A0AEC0] tabular-nums">{entry.progress_percent}%</span>
        </span>
      );
    }
  }
}

export const HistoryEntryRow = memo(function HistoryEntryRow({
  entry,
  variant,
  isLaunching,
  onOpen,
  onPlay,
  onRemove,
}: HistoryEntryRowProps) {
  const movie = isMovie(entry.media_type);
  const capsule = timeCapsuleLabel(entry.time_capsule);
  const canPlay = !!entry.file_path;
  const canResume = canPlay && !movie && entry.resume_position > 0 && entry.watch_state === "partial";
  const thumb = entry.still_path || entry.backdrop_path || entry.poster_path;
  const nested = variant === "nested";

  const primary = nested ? (movie ? entry.show_title : entry.ep_title || `Episode ${entry.ep_num}`) : entry.show_title;
  const secondary = nested
    ? movie ? null : episodeCode(entry)
    : movie ? "Movie" : `${episodeCode(entry)}  ${entry.ep_title || `Episode ${entry.ep_num}`}`;

  return (
    <div
      className={`group/row relative flex items-center gap-4 rounded-xl transition-colors ${
        nested ? "p-2 hover:bg-white/5" : "p-3 bg-[#1F222A]/60 border border-[#2A2D35] hover:bg-[#252830]"
      } ${entry.is_legacy ? "opacity-70" : ""}`}
    >
      <button
        type="button"
        onClick={() => onOpen(entry)}
        className="flex flex-1 min-w-0 items-center gap-4 text-left rounded-lg focus:outline-none focus-visible:ring-2 focus-visible:ring-[#FF6B00]"
        aria-label={`Open ${entry.show_title}${movie ? "" : `, ${episodeCode(entry)}`}`}
      >
        <div
          className={`relative flex-shrink-0 overflow-hidden rounded-lg bg-[#0D0F14] border border-white/5 aspect-video ${
            nested ? "w-28" : "w-36 sm:w-44"
          }`}
        >
          <SafeImage
            srcPath={thumb ? formatImagePath(thumb, "w500") : ""}
            type="backdrop"
            title={entry.show_title}
            altText=""
            className="w-full h-full object-cover"
          />
          {entry.watch_state === "partial" && (
            <div className="absolute bottom-0 left-0 right-0 h-1 bg-black/50" aria-hidden="true">
              <div className="h-full bg-[#FF6B00]" style={{ width: `${Math.min(100, Math.max(2, entry.progress_percent))}%` }} />
            </div>
          )}
        </div>

        <div className="flex-1 min-w-0">
          <p className={`font-bold text-white truncate ${nested ? "text-sm" : "text-base"} group-hover/row:text-[#FF6B00] transition-colors`}>
            {primary}
          </p>
          {secondary && (
            <p className="text-sm text-[#A0AEC0] truncate mt-0.5">
              {secondary}
            </p>
          )}
          <div className="flex items-center gap-x-2 gap-y-1 flex-wrap text-xs text-[#8E929C] mt-1.5">
            {!entry.is_legacy && <span className="tabular-nums">{entry.formatted_time}</span>}
            {!entry.is_legacy && <span aria-hidden="true" className="opacity-40">•</span>}
            <StatusLine entry={entry} />
            {entry.play_count > 1 && (
              <span className="bg-white/10 text-white/80 px-1.5 py-0.5 rounded text-[10px] font-bold tabular-nums" title="Played more than once in this session">
                {entry.play_count}×
              </span>
            )}
            {capsule && !nested && (
              <span
                className={
                  capsule.tone === "early" ? "text-blue-400" : capsule.tone === "premiere" ? "text-[#FF6B00] font-bold" : ""
                }
              >
                • {capsule.text}
              </span>
            )}
            {entry.is_utc_fallback && !entry.is_legacy && (
              <span title="Your system time zone could not be detected; times are shown in UTC.">• UTC</span>
            )}
          </div>
        </div>
      </button>

      <div className="flex items-center gap-1 flex-shrink-0">
        <button
          type="button"
          onClick={() => onRemove(entry)}
          className="w-9 h-9 rounded-full flex items-center justify-center text-white/40 hover:text-red-400 hover:bg-red-500/10 opacity-0 group-hover/row:opacity-100 focus-visible:opacity-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-[#FF6B00] transition-all"
          aria-label={`Remove ${primary} from history`}
          title="Remove from history"
        >
          <Trash2 size={16} aria-hidden="true" />
        </button>

        {canPlay ? (
          <button
            type="button"
            disabled={isLaunching}
            onClick={() => onPlay(entry)}
            className="w-10 h-10 rounded-full bg-[#FF6B00]/15 hover:bg-[#FF6B00] text-[#FF6B00] hover:text-white flex items-center justify-center transition-all disabled:opacity-60 disabled:cursor-wait focus:outline-none focus-visible:ring-2 focus-visible:ring-[#FF6B00] focus-visible:ring-offset-2 focus-visible:ring-offset-[#0D0F14]"
            aria-label={canResume ? `Resume at ${formatClock(entry.resume_position)}` : "Play in VLC"}
            title={canResume ? `Resume at ${formatClock(entry.resume_position)}` : "Play in VLC"}
          >
            {isLaunching ? (
              <Loader2 size={16} className="animate-spin" aria-hidden="true" />
            ) : canResume ? (
              <RotateCcw size={16} aria-hidden="true" />
            ) : (
              <Play size={16} className="ml-0.5 fill-current" aria-hidden="true" />
            )}
          </button>
        ) : (
          <button
            type="button"
            onClick={() => onOpen(entry)}
            className="w-10 h-10 rounded-full flex items-center justify-center text-white/40 hover:text-white hover:bg-white/10 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-[#FF6B00]"
            aria-label="No local file. Open details"
            title="No local file linked. Open details"
          >
            <ChevronRight size={18} aria-hidden="true" />
          </button>
        )}
      </div>
    </div>
  );
});
