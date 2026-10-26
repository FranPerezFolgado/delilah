import { useState } from "react";
import { t, tn } from "../i18n";
import type { UpdateIntent, UpdateState } from "../updates";
import { Markdown } from "./Markdown";
import { Button } from "./ui";
import "./GlobalNotices.css";

interface Props {
  state: UpdateState;
  dispatch: (event: UpdateIntent) => void;
  /** The installed version, shown when up to date. */
  installed?: string;
}

/** A slim, non-blocking banner for the update flow; nothing when idle. */
export function UpdateNotice({ state, dispatch, installed }: Props) {
  const [notesOpen, setNotesOpen] = useState(false);

  switch (state.kind) {
    case "idle":
      return null;
    case "checking":
      // Only a manual check shows anything.
      return state.manual ? <div className="gn-bar" role="status"><span>{t("Buscando actualizaciones…")}</span></div> : null;
    case "upToDate":
      return (
        <div className="gn-bar" role="status">
          <span>{installed ? t("Delilah está al día ({version})", { version: installed }) : t("Delilah está al día")}</span>
          <span className="grow" />
          <Button variant="ghost" size={28} onClick={() => dispatch({ type: "close" })}>{t("Cerrar")}</Button>
        </div>
      );
    case "available":
      return (
        <div className="gn-bar gn-wait" role="status">
          <div className="gn-body">
            <div className="gn-row">
              <span>{t("Delilah {version} disponible", { version: state.version })}</span>
              {state.notes && (
                <Button variant="ghost" size={28} onClick={() => setNotesOpen((o) => !o)}>{t("Abrir novedades")}</Button>
              )}
              <span className="grow" />
              <Button variant="primary" size={28} onClick={() => dispatch({ type: "requestUpdate" })}>{t("Actualizar")}</Button>
              <Button variant="ghost" size={28} onClick={() => dispatch({ type: "later" })}>{t("Más tarde")}</Button>
            </div>
            {notesOpen && state.notes && (
              <div className="gn-notes"><Markdown text={state.notes} remoteImages="click" /></div>
            )}
          </div>
        </div>
      );
    case "downloading": {
      const pct = state.total ? Math.min(100, Math.floor((state.received / state.total) * 100)) : null;
      return (
        <div className="gn-bar gn-wait" role="status">
          <span>{pct === null ? t("Descargando…") : t("Descargando… {pct}%", { pct })}</span>
          <progress className="gn-progress" max={100} value={pct ?? undefined} />
        </div>
      );
    }
    case "installing":
      return <div className="gn-bar gn-wait" role="status"><span>{t("Instalando y reiniciando…")}</span></div>;
    case "error":
      return (
        <div className="gn-bar gn-err" role="alert">
          <span className="gn-dot">●</span>
          <span>{state.version ? t("No se pudo actualizar: {reason}", { reason: state.reason }) : t("No se pudo buscar actualizaciones: {reason}", { reason: state.reason })}</span>
          <span className="grow" />
          <Button variant="primary" size={28} onClick={() => dispatch({ type: "retry" })}>{t("Reintentar")}</Button>
          <Button variant="ghost" size={28} onClick={() => dispatch({ type: "close" })}>{t("Cerrar")}</Button>
        </div>
      );
    case "confirmSessions":
      return (
        <div className="gn-bar gn-wait" role="status">
          <span>{tn(state.running, "Hay {n} sesión en marcha; se detendrá al actualizar", "Hay {n} sesiones en marcha; se detendrán al actualizar")}</span>
          <span className="grow" />
          <Button variant="primary" size={28} onClick={() => dispatch({ type: "now" })}>{t("Actualizar ahora")}</Button>
          <Button variant="ghost" size={28} onClick={() => dispatch({ type: "whenIdle" })}>{t("Cuando terminen")}</Button>
          <Button variant="ghost" size={28} onClick={() => dispatch({ type: "cancel" })}>{t("Cancelar")}</Button>
        </div>
      );
    case "waiting":
      return (
        <div className="gn-bar gn-wait" role="status">
          <span>{tn(state.running, "Se actualizará cuando termine {n} sesión", "Se actualizará cuando terminen {n} sesiones")}</span>
          <span className="grow" />
          <Button variant="primary" size={28} onClick={() => dispatch({ type: "now" })}>{t("Actualizar ahora")}</Button>
          <Button variant="ghost" size={28} onClick={() => dispatch({ type: "cancel" })}>{t("Cancelar")}</Button>
        </div>
      );
  }
}
