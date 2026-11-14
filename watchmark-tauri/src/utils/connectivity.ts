import type { Connectivity } from "../store/useAppStore";

/** Why a TMDB action is unavailable right now, for tooltips and toasts. */
export function tmdbUnavailableReason(connectivity: Connectivity): string {
  return connectivity === "tmdb_blocked"
    ? "TMDB can't be reached on this network. Try Cloudflare WARP or 1.1.1.1 DNS."
    : "You're offline. This needs an internet connection.";
}
