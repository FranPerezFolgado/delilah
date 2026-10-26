import { useEffect, useRef, useState, type ReactNode } from "react";
import { t } from "../i18n";
import { FloorMini } from "./FloorMini";
import { floorOrder } from "./floorLayout";
import { useOffice } from "./useOffice";
import "./Building.css";

interface Props {
  /** A floor was clicked: its owner (a task key or `@project`). */
  onOpenFloor: (owner: string) => void;
  /** The app's connection state, when it already holds the change feed. */
  online?: boolean;
  /** Extra controls at the end of the header. */
  headerExtra?: ReactNode;
}

/** True while the page is hidden, so the building can stop animating. */
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

/** The building: a compact row per floor, newest on top, in a stable order. */
export function Building({ onOpenFloor, online, headerExtra }: Props) {
  const { floors, online: connected } = useOffice(online);
  const hidden = useHidden();
  const order = useRef<string[]>([]);
  order.current = floorOrder(order.current, floors.map((f) => f.owner));
  const rank = new Map(order.current.map((o, i) => [o, i]));
  const sorted = [...floors].sort((a, b) => rank.get(a.owner)! - rank.get(b.owner)!);
  const waiting = floors.reduce((n, f) => n + f.waiting, 0);
  return (
    <div className={`bld${connected ? "" : " bld--stale"}${hidden ? " bld--paused" : ""}`}>
      <div className="view-head" data-tauri-drag-region>
        <h1 data-tauri-drag-region>{t("Playground")}</h1>
        <span className={`bld-count${waiting > 0 ? " bld-count--on" : ""}`}>{t("{n} esperando", { n: waiting })}</span>
        <span className="bld-spacer" />
        {headerExtra}
      </div>
      {!connected && (
        <div className="bld-banner" role="status">
          {t("Reconectando…")}
        </div>
      )}
      <div className="view-body bld-body">
        {sorted.length === 0 ? (
          <p className="bld-empty">{t("Ningún agente trabajando")}</p>
        ) : (
          <div className="bld-floors">
            {sorted.map((f) => (
              <FloorMini key={f.owner} floor={f} onOpen={onOpenFloor} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
