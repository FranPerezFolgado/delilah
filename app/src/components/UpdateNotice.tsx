import { useState } from "react";
import { t, tn } from "../i18n";
import type { UpdateIntent, UpdateState } from "../updates";
import { Markdown } from "./Markdown";

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
      return state.manual ? <div className="notice update-notice"><span>{t("Buscando actualizaciones…")}</span></div> : null;
    case "upToDate":
      return (
        <div className="notice update-notice">
          <span>{installed ? t("Delilah está al día ({version})", { version: installed }) : t("Delilah está al día")}</span>
          <span className="grow" />
          <button className="btn btn-ghost" onClick={() => dispatch({ type: "close" })}>{t("Cerrar")}</button>
        </div>
      );
    case "available":
      return (
        <div className="notice update-notice wait">
          <div className="update-body">
            <div className="update-row">
              <span>{t("Delilah {version} disponible", { version: state.version })}</span>
              {state.notes && (
                <button className="btn btn-ghost" onClick={() => setNotesOpen((o) => !o)}>{t("Ver novedades")}</button>
              )}
              <span className="grow" />
              <button className="btn btn-p" onClick={() => dispatch({ type: "requestUpdate" })}>{t("Actualizar")}</button>
              <button className="btn btn-ghost" onClick={() => dispatch({ type: "later" })}>{t("Más tarde")}</button>
            </div>
            {notesOpen && state.notes && (
              <div className="update-notes"><Markdown text={state.notes} remoteImages="click" /></div>
            )}
          </div>
        </div>
      );
    case "downloading": {
      const pct = state.total ? Math.min(100, Math.floor((state.received / state.total) * 100)) : null;
      return (
        <div className="notice update-notice wait">
          <span>{pct === null ? t("Descargando…") : t("Descargando… {pct}%", { pct })}</span>
          <progress className="update-progress" max={100} value={pct ?? undefined} />
        </div>
      );
    }
    case "installing":
      return <div className="notice update-notice wait"><span>{t("Instalando y reiniciando…")}</span></div>;
    case "error":
      return (
        <div className="notice update-notice" style={{ borderColor: "var(--err)" }}>
          <span className="c-err">●</span>
          <span>{state.version ? t("No se pudo actualizar: {reason}", { reason: state.reason }) : t("No se pudo buscar actualizaciones: {reason}", { reason: state.reason })}</span>
          <span className="grow" />
          <button className="btn btn-p" onClick={() => dispatch({ type: "retry" })}>{t("Reintentar")}</button>
          <button className="btn btn-ghost" onClick={() => dispatch({ type: "close" })}>{t("Cerrar")}</button>
        </div>
      );
    case "confirmSessions":
      return (
        <div className="notice update-notice wait">
          <span>{tn(state.running, "Hay {n} sesión en marcha; se detendrá al actualizar", "Hay {n} sesiones en marcha; se detendrán al actualizar")}</span>
          <span className="grow" />
          <button className="btn btn-p" onClick={() => dispatch({ type: "now" })}>{t("Actualizar ahora")}</button>
          <button className="btn btn-ghost" onClick={() => dispatch({ type: "whenIdle" })}>{t("Cuando terminen")}</button>
          <button className="btn btn-ghost" onClick={() => dispatch({ type: "cancel" })}>{t("Cancelar")}</button>
        </div>
      );
    case "waiting":
      return (
        <div className="notice update-notice wait">
          <span>{tn(state.running, "Se actualizará cuando termine {n} sesión", "Se actualizará cuando terminen {n} sesiones")}</span>
          <span className="grow" />
          <button className="btn btn-p" onClick={() => dispatch({ type: "now" })}>{t("Actualizar ahora")}</button>
          <button className="btn btn-ghost" onClick={() => dispatch({ type: "cancel" })}>{t("Cancelar")}</button>
        </div>
      );
  }
}
