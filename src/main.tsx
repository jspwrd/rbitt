// Must be first: installs the browser-only IPC mock before any Tauri API use.
import "./mocks/tauri-mock";
import React from "react";

// The toolbar doubles as the macOS titlebar (overlay traffic lights), so it
// needs left clearance — but only in the real app on macOS, not in browser
// harness runs (which set __RBITT_MOCK__).
if (
  navigator.platform.startsWith("Mac") &&
  !("__RBITT_MOCK__" in window)
) {
  document.documentElement.classList.add("platform-macos");
}
import ReactDOM from "react-dom/client";
import App from "./App";
import { ErrorBoundary } from "./components";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <App />
    </ErrorBoundary>
  </React.StrictMode>,
);
