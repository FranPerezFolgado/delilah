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
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name, className }: { name: IconName; className?: string }) {
  return (
    <svg className={`i ${className ?? ""}`} viewBox="0 0 24 24" aria-hidden="true">
      <path d={PATHS[name]} />
    </svg>
  );
}
