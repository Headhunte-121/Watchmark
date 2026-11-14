import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openUrl } from "@tauri-apps/plugin-shell";
import { AnimatePresence, motion } from "framer-motion";
import { CloudOff, ExternalLink, Loader2, RefreshCw, ShieldAlert, X } from "lucide-react";
import { useAppStore, type Connectivity, type ConnectivityReport } from "../store/useAppStore";
import { logger } from "../utils/logger";
import { toast } from "../utils/toast";

const WARP_URL = "https://one.one.one.one/";

const isDegraded = (s: Connectivity) => s === "tmdb_blocked" || s === "offline";

/**
 * Keeps the app's connectivity state in sync with the backend's TMDB checks
 * and explains what's wrong when TMDB can't be reached:
 * - "tmdb_blocked": the internet works but TMDB doesn't (common on some ISPs,
 *   such as Jio). Suggests Cloudflare WARP / 1.1.1.1.
 * - "offline": no internet at all.
 */
export function ConnectivityBanner() {
  const connectivity = useAppStore(s => s.connectivity);
  const setConnectivity = useAppStore(s => s.setConnectivity);
  const [dismissedFor, setDismissedFor] = useState<Connectivity | null>(null);
  const [checking, setChecking] = useState(false);
  const previous = useRef<Connectivity>("unknown");

  const applyReport = (report: ConnectivityReport | null | undefined) => {
    if (!report?.status) return;
    const before = previous.current;
    previous.current = report.status;
    setConnectivity(report.status);
    if (isDegraded(before) && report.status === "online") {
      toast.success("Connected to TMDB again. Syncing your shows.");
    }
  };

  const recheck = async (source: string) => {
    setChecking(true);
    try {
      applyReport(await invoke<ConnectivityReport>("check_connectivity"));
    } catch (e) {
      logger.error(`Connectivity check failed (${source})`, e);
    } finally {
      setChecking(false);
    }
  };

  useEffect(() => {
    recheck("startup");
    const unlisten = listen<ConnectivityReport>("network-status", event => applyReport(event.payload));
    // The OS knows about lost/regained connections before our heartbeat does.
    const onOffline = () => {
      previous.current = "offline";
      setConnectivity("offline");
    };
    const onOnline = () => recheck("os-online");
    window.addEventListener("offline", onOffline);
    window.addEventListener("online", onOnline);
    return () => {
      unlisten.then(fn => fn()).catch(() => {});
      window.removeEventListener("offline", onOffline);
      window.removeEventListener("online", onOnline);
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // A new problem (or a recovery) clears an earlier dismissal.
  useEffect(() => {
    if (dismissedFor && dismissedFor !== connectivity) setDismissedFor(null);
  }, [connectivity, dismissedFor]);

  const visible = isDegraded(connectivity) && dismissedFor !== connectivity;
  const blocked = connectivity === "tmdb_blocked";

  return (
    <AnimatePresence>
      {visible && (
        <motion.div
          key={connectivity}
          role="status"
          aria-live="polite"
          initial={{ y: 24, opacity: 0 }}
          animate={{ y: 0, opacity: 1 }}
          exit={{ y: 24, opacity: 0 }}
          transition={{ duration: 0.2 }}
          className="fixed bottom-6 left-1/2 -translate-x-1/2 lg:left-[calc(50%+128px)] z-[60] w-[min(36rem,calc(100vw-2rem))] rounded-2xl border border-amber-500/30 bg-[#1F222A]/95 backdrop-blur-md shadow-2xl p-4"
        >
          <div className="flex gap-3">
            <div className="flex-shrink-0 w-9 h-9 rounded-full bg-amber-500/15 text-amber-400 flex items-center justify-center">
              {blocked ? <ShieldAlert size={18} aria-hidden="true" /> : <CloudOff size={18} aria-hidden="true" />}
            </div>
            <div className="flex-1 min-w-0">
              <p className="font-bold text-white">
                {blocked ? "Can't reach TMDB on this network" : "You're offline"}
              </p>
              <p className="text-sm text-[#A0AEC0] mt-1 leading-relaxed">
                {blocked ? (
                  <>
                    Your internet is working, but TMDB isn't responding. Some providers, such as Jio, block it. Turn on{" "}
                    <span className="text-white font-semibold">Cloudflare WARP</span> (the free 1.1.1.1 app) or set your
                    DNS to <span className="text-white font-semibold tabular-nums">1.1.1.1</span>, then retry.
                  </>
                ) : (
                  <>Searching and updating show info will resume when you're back online.</>
                )}{" "}
                Your library, history and playback keep working.
              </p>
              <div className="flex flex-wrap items-center gap-2 mt-3">
                <button
                  type="button"
                  onClick={() => {
                    logger.click("Retry TMDB connection");
                    recheck("retry");
                  }}
                  disabled={checking}
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-[#FF6B00] hover:bg-[#FF8533] text-white text-sm font-bold transition-colors disabled:opacity-60 disabled:cursor-wait focus:outline-none focus-visible:ring-2 focus-visible:ring-white"
                >
                  {checking ? <Loader2 size={14} className="animate-spin" aria-hidden="true" /> : <RefreshCw size={14} aria-hidden="true" />}
                  {checking ? "Checking…" : "Retry"}
                </button>
                {blocked && (
                  <button
                    type="button"
                    onClick={() => {
                      logger.click("Open Cloudflare WARP page");
                      openUrl(WARP_URL).catch(e => logger.error("Failed to open WARP page", e));
                    }}
                    className="flex items-center gap-1.5 px-3 py-1.5 rounded-full bg-white/10 hover:bg-white/20 text-white text-sm font-bold transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-white"
                  >
                    Get Cloudflare WARP <ExternalLink size={13} aria-hidden="true" />
                  </button>
                )}
              </div>
            </div>
            <button
              type="button"
              onClick={() => setDismissedFor(connectivity)}
              className="flex-shrink-0 self-start w-8 h-8 rounded-full flex items-center justify-center text-white/40 hover:text-white hover:bg-white/10 transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-white"
              aria-label="Hide this message"
              title="Hide until the connection changes"
            >
              <X size={16} aria-hidden="true" />
            </button>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
