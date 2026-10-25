const C = "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18z";

const PATHS = {
  run: C + "M10 8.6l5 3.4-5 3.4z",
  wait: C + "M9.6 9.4a2.5 2.5 0 1 1 3.4 2.3c-.6.3-1 .8-1 1.4v.4M12 16.6v.1",
  pause: C + "M10 9v6M14 9v6",
  done: C + "M8.3 12.4l2.5 2.5 5-5.4",
  failed: C + "M9.5 9.5l5 5M14.5 9.5l-5 5",
  tasks: "M10 6h10M10 12h10M10 18h10M4 6l1.2 1.2L7.5 5M4 12l1.2 1.2 2.3-2.2M4 18l1.2 1.2 2.3-2.2",
  console: "M4 5h16v14H4zM8 10l3 2-3 2M13 15h3",
  folder: "M3 7h6l2 2h10v10H3z",
  plus: "M12 5v14M5 12h14",
  open: "M14 4h6v6M20 4l-8 8M18 14v6H4V6h6",
  maximize: "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5",
  ticket: "M4 6h16v4a2 2 0 0 0 0 4v4H4v-4a2 2 0 0 0 0-4zM14 6v12",
  refresh: "M20 12a8 8 0 1 1-2.3-5.7M20 4v4h-4",
  close: "M7 7l10 10M17 7L7 17",
  settings: "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z",
  review: "M4 6h16M4 12h10M4 18h7M16 15l2 2 4-4",
  diff: "M8 4v8M4 8h8M12 19h8M18 4L6 20",
  back: "M15 6l-6 6 6 6",
  code: "M8 7l-5 5 5 5M16 7l5 5-5 5M14 4l-4 16",
  sidebar: "M4 5h16v14H4zM15 5v14",
  lock: "M7 11V8a5 5 0 0 1 10 0v3M6 11h12v9H6z",
  inbox: "M4 13l2.5-7h11l2.5 7v6H4zM4 13h5l1 2h4l1-2h5",
  trash: "M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3",
  idea: "M9 18h6M10 21h4M12 3a6 6 0 0 0-3.5 10.9c.6.5 1 1.2 1 2.1h5c0-.9.4-1.6 1-2.1A6 6 0 0 0 12 3z",
  backlog: C + "M12 7.5V12l3 2",
} as const;

/** The v2 set: viewBox 16, stroke 1.5 (the 24-box icons above are the older ones). */
const PATHS16 = {
  rail: "M2.5 3.5h11v9h-11zM6 3.5v9",
  logs: "M2.5 3.5h11v9h-11zM5 6.5l2 1.5-2 1.5M8.5 10h2.5",
  more: "M3.5 8h.01M8 8h.01M12.5 8h.01",
  repo: "M4 2.5h8v11H4zM4 11h8M6.5 5h3",
  pr: "M4.5 4.5v7M4.5 4.5a1.2 1.2 0 1 0 0-.01zM4.5 11.5a1.2 1.2 0 1 0 0 .01zM11.5 11.5a1.2 1.2 0 1 0 0 .01zM11.5 10.3V6.5a2 2 0 0 0-2-2H8M9.5 3l-1.7 1.5L9.5 6",
  merge: "M4.5 4.5v7M4.5 4.5a1.2 1.2 0 1 0 0-.01zM4.5 11.5a1.2 1.2 0 1 0 0 .01zM11.5 9.5a1.2 1.2 0 1 0 0 .01zM4.5 8.5c0-2 1.5-2.5 3-2.5s4 .5 4 2.3",
  check: "M3.5 8.5l3 3 6-6.5",
  chevron: "M6 3.5L10.5 8 6 12.5",
  filter: "M2.5 4h11M4.5 8h7M6.5 12h3",
  external: "M9 2.5h4.5V7M13.5 2.5L7.5 8.5M11.5 9.5v4h-9v-9h4",
  retry: "M13 8a5 5 0 1 1-1.5-3.6M13 2.5v3h-3",
  priority: "M8 2.5v8M8 13.5v.01",
  autopilot: "M2.5 9.5l11-6-4 10-2-4z",
} as const;

export type IconName = keyof typeof PATHS | keyof typeof PATHS16;

const is16 = (n: IconName): n is keyof typeof PATHS16 => n in PATHS16;

export function Icon({ name, className }: { name: IconName; className?: string }) {
  const small = is16(name);
  return (
    <svg className={`i ${className ?? ""}`} viewBox={small ? "0 0 16 16" : "0 0 24 24"} strokeWidth={small ? 1.5 : undefined} aria-hidden="true">
      <path d={small ? PATHS16[name] : PATHS[name]} />
    </svg>
  );
}
