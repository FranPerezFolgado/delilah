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
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, className }: { name: IconName; className?: string }) {
  return (
    <svg className={`i ${className ?? ""}`} viewBox="0 0 24 24" aria-hidden="true">
      <path d={PATHS[name]} />
    </svg>
  );
}
