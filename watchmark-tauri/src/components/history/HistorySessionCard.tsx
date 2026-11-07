import { memo, useId, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import { ChevronDown, Moon, Trash2 } from "lucide-react";
import { SafeImage } from "../ui/SafeImage";
import { formatImagePath } from "../../utils/imageFormat";
import type { HistoryEntry, HistoryGroup } from "./historyTypes";
import { formatDuration, sessionRange, sessionVibe } from "./historyFormat";
import { HistoryEntryRow } from "./HistoryEntryRow";

interface HistorySessionCardProps {
  group: HistoryGroup;
  launchingId: number | null;
  onOpen: (entry: HistoryEntry) => void;
  onPlay: (entry: HistoryEntry) => void;
  onRemoveEntry: (entry: HistoryEntry) => void;
  onRemoveGroup: (group: HistoryGroup) => void;
}

/** A binge session: several episodes of one show watched in one sitting. */
export const HistorySessionCard = memo(function HistorySessionCard({
  group,
  launchingId,
  onOpen,
  onPlay,
  onRemoveEntry,
  onRemoveGroup,
}: HistorySessionCardProps) {
  const [isOpen, setIsOpen] = useState(false);
  const reduceMotion = useReducedMotion();
  const panelId = useId();
  const lead = group.entries[0];
  if (!lead) return null;

  const { label: vibe, crossesMidnight } = sessionVibe(group);
  const thumb = lead.backdrop_path || lead.still_path || lead.poster_path;

  return (
    <div className={`group/session rounded-xl bg-[#1F222A]/60 border border-[#2A2D35] ${group.is_legacy ? "opacity-70" : ""}`}>
      <div className="relative flex items-center gap-4 p-3 rounded-xl hover:bg-[#252830] transition-colors">
        <button
          type="button"
          onClick={() => setIsOpen(o => !o)}
          aria-expanded={isOpen}
          aria-controls={panelId}
          className="flex flex-1 min-w-0 items-center gap-4 text-left rounded-lg focus:outline-none focus-visible:ring-2 focus-visible:ring-[#FF6B00]"
        >
          <div className="relative w-36 sm:w-44 aspect-video flex-shrink-0 overflow-hidden rounded-lg bg-[#0D0F14] border border-white/5">
            <SafeImage
              srcPath={thumb ? formatImagePath(thumb, "w500") : ""}
              type="backdrop"
              title={group.show_title}
              altText=""
              className="w-full h-full object-cover"
            />
            <div className="absolute inset-0 bg-gradient-to-t from-black/80 via-black/10 to-transparent" aria-hidden="true" />
            <span className="absolute bottom-1.5 left-2 text-[11px] font-bold text-white tabular-nums">
              {group.episode_count} episodes
            </span>
          </div>

          <div className="flex-1 min-w-0">
            <p className="text-base font-bold text-white truncate group-hover/session:text-[#FF6B00] transition-colors">
              {group.show_title}
            </p>
            <p className="text-sm text-[#A0AEC0] truncate mt-0.5">{sessionRange(group)}</p>
            <div className="flex items-center gap-2 flex-wrap text-xs text-[#8E929C] mt-1.5">
              <span className="bg-[#FF6B00]/15 text-[#FF6B00] font-bold px-2 py-0.5 rounded-full border border-[#FF6B00]/25">
                Binge session
              </span>
              {!group.is_legacy && (
                <span className="flex items-center gap-1">
                  {vibe}
                  {crossesMidnight && <Moon size={12} aria-label="Crossed midnight" />}
                </span>
              )}
              <span aria-hidden="true" className="opacity-40">•</span>
              <span className="tabular-nums">{formatDuration(group.watched_minutes)}</span>
            </div>
          </div>

          <motion.span
            animate={{ rotate: isOpen ? 180 : 0 }}
            transition={{ duration: reduceMotion ? 0 : 0.2 }}
            className="flex-shrink-0 text-white/50"
            aria-hidden="true"
          >
            <ChevronDown size={22} />
          </motion.span>
        </button>

        <button
          type="button"
          onClick={() => onRemoveGroup(group)}
          className="w-9 h-9 flex-shrink-0 rounded-full flex items-center justify-center text-white/40 hover:text-red-400 hover:bg-red-500/10 opacity-0 group-hover/session:opacity-100 focus-visible:opacity-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-[#FF6B00] transition-all"
          aria-label={`Remove this ${group.show_title} session from history`}
          title="Remove session from history"
        >
          <Trash2 size={16} aria-hidden="true" />
        </button>
      </div>

      <AnimatePresence initial={false}>
        {isOpen && (
          <motion.div
            id={panelId}
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ duration: reduceMotion ? 0 : 0.25, ease: "easeInOut" }}
            className="overflow-hidden"
          >
            <div className="px-3 pb-3 pt-1 border-t border-white/5 flex flex-col gap-1">
              {group.entries.map(entry => (
                <HistoryEntryRow
                  key={entry.hist_id}
                  entry={entry}
                  variant="nested"
                  isLaunching={launchingId === entry.episode_id}
                  onOpen={onOpen}
                  onPlay={onPlay}
                  onRemove={onRemoveEntry}
                />
              ))}
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
});
