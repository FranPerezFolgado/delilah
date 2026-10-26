// Connection to the daemon: down on a failed request, then health is polled every 2 s,
// backing off to at most 5 s (never longer, so it recovers within 5 s: SC-009).
import { useCallback, useEffect, useRef, useState } from "react";
import { api, onRequestResult } from "./api";

export type ConnectionStatus = "up" | "down";

export const POLL_MIN_MS = 2000;
export const POLL_MAX_MS = 5000;

export interface Connection {
  status: ConnectionStatus;
  /** Epoch ms of the last successful request, or null if none yet. */
  lastOk: number | null;
  /** Checks health now instead of waiting for the next poll. */
  retry: () => void;
}

export function useConnection(): Connection {
  const [status, setStatus] = useState<ConnectionStatus>("up");
  const [lastOk, setLastOk] = useState<number | null>(null);
  const down = useRef(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const delay = useRef(POLL_MIN_MS);
  const alive = useRef(true);

  const markUp = useCallback((at: number) => {
    down.current = false;
    clearTimeout(timer.current);
    delay.current = POLL_MIN_MS;
    setStatus("up");
    setLastOk(at);
  }, []);

  const poll = useCallback(async () => {
    clearTimeout(timer.current);
    try {
      await api.health();
      if (alive.current) markUp(Date.now());
    } catch {
      if (!alive.current || !down.current) return;
      delay.current = Math.min(POLL_MAX_MS, Math.round(delay.current * 1.5));
      timer.current = setTimeout(poll, delay.current);
    }
  }, [markUp]);

  useEffect(() => {
    alive.current = true;
    const off = onRequestResult((ok, at) => {
      if (ok) return markUp(at);
      if (down.current) return;
      down.current = true;
      setStatus("down");
      delay.current = POLL_MIN_MS;
      timer.current = setTimeout(poll, POLL_MIN_MS);
    });
    return () => {
      alive.current = false;
      off();
      clearTimeout(timer.current);
    };
  }, [markUp, poll]);

  const retry = useCallback(() => void poll(), [poll]);
  return { status, lastOk, retry };
}
