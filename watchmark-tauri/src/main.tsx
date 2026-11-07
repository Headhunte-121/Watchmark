import React from "react";
import ReactDOM from "react-dom/client";
import "sonner/dist/styles.css";
import "./index.css";
import App from "./App";
import { ErrorBoundary } from "./components/ui/ErrorBoundary";
import { logger } from "./utils/logger";

// Errors outside React rendering (event handlers, timers, promises) never reach
// an error boundary; make sure they at least land in WatchMark.log.
window.addEventListener("error", (event) => {
  logger.error(`Uncaught error: ${event.message}`, { source: event.filename, line: event.lineno, stack: event.error?.stack });
});
window.addEventListener("unhandledrejection", (event) => {
  const reason = event.reason;
  logger.error(`Unhandled promise rejection: ${reason?.message ?? String(reason)}`, { stack: reason?.stack });
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary scope="app" variant="app">
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
