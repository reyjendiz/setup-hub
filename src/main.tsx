import "@fontsource-variable/inter";
import "./index.css";
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";

// No browser context menu / reload shortcuts in a desktop app.
window.addEventListener("contextmenu", (e) => {
  if (!(e.target as HTMLElement).closest("input,textarea,.selectable")) e.preventDefault();
});

if (import.meta.env.DEV && !("__TAURI_INTERNALS__" in window && (window as any).__TAURI_INTERNALS__.metadata)) await import("./devmock");

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
