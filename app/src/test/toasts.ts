import { waitFor, within } from "@testing-library/react";

/** Queries scoped to the visible toast stack, leaving out the screen-reader announcer
 *  (`.toast-sr`), which repeats the latest toast text. Empty when no host is mounted. */
export function inToasts() {
  return within((document.body.querySelector(".toast-host") as HTMLElement | null) ?? document.createElement("div"));
}

/** Text of the visible toast stack only. */
export const toastsText = () => document.body.querySelector(".toast-host")?.textContent ?? "";

/** Waits for the first visible toast and returns it. */
export const findToast = () =>
  waitFor(() => {
    const el = document.body.querySelector<HTMLElement>(".toast-host .toast");
    if (!el) throw new Error("no toast yet");
    return el;
  });
