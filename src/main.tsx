import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { CrashBoundary } from "./components/CrashBoundary";
import { primeStartupBootstrap } from "./lib/local";
import "./styles.css";

// This command is a coherent read only. Starting it before React commits the
// safe shell overlaps IPC/SQLite latency without opening the recovery fence or
// enabling an upload action; App awaits this same singleflight promise.
void primeStartupBootstrap();
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <CrashBoundary>
      <App />
    </CrashBoundary>
  </StrictMode>,
);

// Harness instrumentation is never allowed to delay the first safe-shell
// render. Buffered long-task observation and the real Batch-content marker can
// attach after their tiny harness-only chunk arrives.
if (import.meta.env.TAURI_ENV_PERFORMANCE_HARNESS === "1") {
  void import("./performance-harness")
    .then(({ initializePerformanceHarness }) => {
      initializePerformanceHarness();
    })
    .catch(() => {
      // Instrumentation failure must not interfere with the operator workspace.
    });
}
