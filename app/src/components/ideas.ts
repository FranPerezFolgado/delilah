import type { Session } from "../api";

/** Which idea conversation the sidebar "Ideas" row opens: the first one waiting
 *  for the user, else the one last shown (if still listed), else the newest.
 *  "Newest" is the last listed because the daemon lists sessions in creation
 *  order. `ideas` must not be empty. */
export function ideaToOpen(ideas: Session[], lastShown: string | null): Session {
  return (
    ideas.find((s) => s.state === "waiting") ??
    ideas.find((s) => s.id === lastShown) ??
    ideas[ideas.length - 1]
  );
}
