import { Sparkles } from "lucide-react";
import { cn } from "../../utils/cn";

interface NewSeasonBadgeProps {
  /** Season number that premiered recently (from the backend's `new_season`). */
  season: number | null | undefined;
  className?: string;
}

/**
 * "New Season" pill for posters. Renders nothing unless the backend flagged a
 * season (2 or later) that premiered within the last 30 days.
 */
export function NewSeasonBadge({ season, className }: NewSeasonBadgeProps) {
  if (!season || season < 2) return null;
  return (
    <div
      className={cn(
        "flex items-center gap-1 px-2 py-1 bg-[#FF6B00] rounded-md text-[10px] font-bold text-white shadow-md uppercase tracking-wider",
        className,
      )}
      title={`Season ${season} premiered recently`}
      aria-label={`New season: season ${season}`}
    >
      <Sparkles className="w-3 h-3" aria-hidden="true" />
      New Season
    </div>
  );
}
