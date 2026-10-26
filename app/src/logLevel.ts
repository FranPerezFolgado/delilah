// Level of each plain-text daemon log line (R21): the tracing level token, inherited by
// continuation lines of a multi-line message.
export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";
export type LogFilter = "all" | "info" | "warn" | "error";

const TOKEN = /(?:^|[\s\[])(TRACE|DEBUG|INFO|WARN|ERROR)(?=[\s\]:]|$)/;

/** The level named in `line`, or null if it has none. Only looks at the line's head (timestamp, level, target). */
export function levelOf(line: string): LogLevel | null {
  const m = TOKEN.exec(line.slice(0, 80));
  return m ? (m[1].toLowerCase() as LogLevel) : null;
}

/** One entry per line, in order; a line without a level takes the previous line's (null before any). */
export function parseLogs(text: string): { line: string; level: LogLevel | null }[] {
  let last: LogLevel | null = null;
  return text.split("\n").map((line) => {
    const own = levelOf(line);
    if (own) last = own;
    return { line, level: own ?? last };
  });
}

const RANK: Record<LogLevel, number> = { trace: 0, debug: 1, info: 2, warn: 3, error: 4 };
const MIN: Record<LogFilter, number> = { all: -1, info: RANK.info, warn: RANK.warn, error: RANK.error };

/** Whether a line of `level` passes `filter`: «Info» shows info and above, «Advertencias» warn and above, «Errores» only errors. */
export function passes(level: LogLevel | null, filter: LogFilter): boolean {
  if (filter === "all") return true;
  return level !== null && RANK[level] >= MIN[filter];
}
