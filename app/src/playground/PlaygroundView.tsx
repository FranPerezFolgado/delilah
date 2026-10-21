import { useEffect, useState } from "react";
import { t } from "../i18n";
import { openPlaygroundWindow } from "../platform";
import "./playground.css";
import { Desk } from "./Desk";
import { deskState, type DeskModel } from "./layout";
import { usePlayground } from "./usePlayground";

interface Props {
  onOpen: (task: string, session: string) => void;
  /** The app's connection state, when it already holds the change feed. */
  online?: boolean;
  /** Already in its own window: no "open in window" button. */
  standalone?: boolean;
}

/** One cell per slot, 0..max: a freed slot stays an empty cell so no desk moves. */
function cells(desks: DeskModel[]): (DeskModel | undefined)[] {
  const out: (DeskModel | undefined)[] = Array(Math.max(-1, ...desks.map((d) => d.slot)) + 1).fill(undefined);
  for (const d of desks) out[d.slot] = d;
  return out;
}

/** True while the page is hidden, so the office can stop animating. */
function useHidden(): boolean {
  const [hidden, setHidden] = useState(() => document.hidden);
  useEffect(() => {
    const on = () => setHidden(document.hidden);
    on();
    document.addEventListener("visibilitychange", on);
    return () => document.removeEventListener("visibilitychange", on);
  }, []);
  return hidden;
}

/** The office: an area per project with a desk for each live agent. */
export function PlaygroundView({ onOpen, online, standalone }: Props) {
  const { areas, online: connected } = usePlayground(online);
  const hidden = useHidden();
  const waiting = areas.reduce((n, a) => n + a.desks.filter((d) => deskState(d.session) === "waiting").length, 0);
  return (
    <div className={`pg-office${connected ? "" : " pg-office--stale"}${hidden ? " pg-office--paused" : ""}`}>
      {!connected && (
        <div className="pg-banner" role="status">
          {t("Reconectando…")}
        </div>
      )}
      <header className="pg-head">
        <h2>{t("Playground")}</h2>
        <span className={`pg-count${waiting > 0 ? " pg-count--on" : ""}`}>{t("{n} esperando", { n: waiting })}</span>
        {!standalone && (
          <button type="button" className="pg-window" style={{ marginLeft: "auto" }} onClick={() => void openPlaygroundWindow()}>
            {t("Abrir en ventana")}
          </button>
        )}
      </header>
      {areas.length === 0 ? (
        <p className="pg-empty">{t("Ningún agente trabajando")}</p>
      ) : (
        <div className="pg-areas">
          {areas.map((a) => (
            <section key={a.project} className="pg-area">
              <h3>{a.project}</h3>
              <div className="pg-grid">
                {cells(a.desks).map((d, i) =>
                  d ? <Desk key={d.session.id} desk={d} onOpen={onOpen} /> : <div key={`free-${i}`} className="pg-desk pg-desk--empty" aria-hidden="true" />,
                )}
              </div>
            </section>
          ))}
        </div>
      )}
    </div>
  );
}
