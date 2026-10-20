import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import "./styles.css";

// A native app has no "Inspect Element": no context menu, except where it is useful
// (copy and paste in fields, the editor and terminals). ⌥ + right click still opens
// it, for debugging. Release builds carry no inspector at all.
window.addEventListener("contextmenu", (e) => {
  if (e.altKey) return;
  const target = e.target as Element | null;
  if (target?.closest("input, textarea, [contenteditable='true'], .cm-editor, .xterm")) return;
  e.preventDefault();
});

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
