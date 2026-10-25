import "./Skeleton.css";

export interface SkeletonProps {
  lines?: number;
  /** px number or any CSS length, applied to every line. */
  width?: number | string;
}

/** Placeholder lines for content that is loading. */
export function Skeleton({ lines = 1, width }: SkeletonProps) {
  const w = typeof width === "number" ? `${width}px` : width;
  const ref = (el: HTMLElement | null) => {
    if (el && w) el.style.width = w;
  };
  return (
    <div className="sk-group" aria-busy="true">
      {Array.from({ length: lines }, (_, i) => (
        <span key={i} className="sk" ref={ref} />
      ))}
    </div>
  );
}
