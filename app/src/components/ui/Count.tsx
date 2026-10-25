import "./Count.css";

export interface CountProps {
  n: number;
  /** Something waits on the user: `--wait` background. */
  hot?: boolean;
  showZero?: boolean;
}

export function Count({ n, hot, showZero }: CountProps) {
  if (n === 0 && !showZero) return null;
  return <span className={hot ? "cnt hot" : "cnt"}>{n}</span>;
}
