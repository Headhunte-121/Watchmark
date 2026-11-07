import { Component, type ErrorInfo, type ReactNode } from "react";
import { AlertTriangle, RefreshCw, RotateCcw } from "lucide-react";
import { logger } from "../../utils/logger";

interface ErrorBoundaryProps {
  /** Shown in the log and the fallback so the failing area is identifiable. */
  scope: string;
  /** "view" renders an in-page panel; "app" renders a full-window screen. */
  variant?: "view" | "app";
  children: ReactNode;
}

interface ErrorBoundaryState {
  error: Error | null;
}

/**
 * Catches render errors so one broken view can't unmount the whole app.
 * Errors are forwarded to the backend log (WatchMark.log) via the logger.
 */
export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    logger.error(`Render crash in ${this.props.scope}: ${error.message}`, {
      stack: error.stack,
      componentStack: info.componentStack,
    });
  }

  private retry = () => {
    logger.click(`Retry after crash in ${this.props.scope}`);
    this.setState({ error: null });
  };

  private reload = () => {
    logger.click("Reload app after crash");
    window.location.reload();
  };

  render() {
    const { error } = this.state;
    if (!error) return this.props.children;

    const isApp = this.props.variant === "app";
    return (
      <div
        role="alert"
        className={`flex flex-col items-center justify-center text-center px-6 ${
          isApp ? "h-screen w-screen bg-[#0D0F14]" : "h-full w-full py-24"
        }`}
      >
        <AlertTriangle size={56} className="text-[#FF6B00]/70 mb-5" aria-hidden="true" />
        <h2 className="text-2xl font-bold text-white mb-2">
          {isApp ? "WatchMark ran into a problem" : `The ${this.props.scope} couldn't be displayed`}
        </h2>
        <p className="text-[#8E929C] max-w-md mb-1">
          Your library and history are safe. The error has been written to WatchMark.log.
        </p>
        <p className="text-xs text-white/30 max-w-lg mb-6 break-words font-mono">{error.message}</p>
        <div className="flex gap-3">
          {!isApp && (
            <button
              type="button"
              onClick={this.retry}
              className="flex items-center gap-2 px-5 py-2.5 rounded-full bg-[#FF6B00] hover:bg-[#FF8533] text-white font-bold transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-white"
            >
              <RotateCcw size={16} aria-hidden="true" /> Try again
            </button>
          )}
          <button
            type="button"
            onClick={this.reload}
            className={`flex items-center gap-2 px-5 py-2.5 rounded-full font-bold transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-white ${
              isApp ? "bg-[#FF6B00] hover:bg-[#FF8533] text-white" : "bg-white/10 hover:bg-white/20 text-white"
            }`}
          >
            <RefreshCw size={16} aria-hidden="true" /> Reload app
          </button>
        </div>
      </div>
    );
  }
}
