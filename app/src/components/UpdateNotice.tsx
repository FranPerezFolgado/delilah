import { useState } from "react";
import { t, tn } from "../i18n";
import type { UpdateIntent, UpdateState } from "../updates";
import { Markdown } from "./Markdown";
import { Button, Notice, useDismissal } from "./ui";
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
  // Kinds with nothing to dispatch are only hidden, until the kind (or version) changes.
  const hideOnly = state.kind === "checking" ? state.manual : state.kind === "downloading" || state.kind === "installing" || state.kind === "waiting";
  const sig = hideOnly ? `${state.kind}:${"version" in state ? state.version : ""}` : null;
  const [hidden, hide] = useDismissal(sig);
  if (hidden) return null;

  switch (state.kind) {
    case "idle":
      return null;
    case "checking":
      // Only a manual check shows anything.
      return state.manual ? <Notice className="gn-bar" role="status" onClose={hide}><span>{t("Buscando actualizaciones…")}</span></Notice> : null;
    case "upToDate":
      return (
        <Notice className="gn-bar" role="status" onClose={() => dispatch({ type: "close" })}>
          <span>{installed ? t("Delilah está al día ({version})", { version: installed }) : t("Delilah está al día")}</span>
          <span className="grow" />
        </Notice>
      );
    case "available":
      return (
        <Notice className="gn-bar gn-wait" role="status" onClose={() => dispatch({ type: "later" })}>
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
        </Notice>
      );
    case "downloading": {
      const pct = state.total ? Math.min(100, Math.floor((state.received / state.total) * 100)) : null;
      return (
        <Notice className="gn-bar gn-wait" role="status" onClose={hide}>
          <span>{pct === null ? t("Descargando…") : t("Descargando… {pct}%", { pct })}</span>
          <progress className="gn-progress" max={100} value={pct ?? undefined} />
        </Notice>
      );
    }
    case "installing":
      return <Notice className="gn-bar gn-wait" role="status" onClose={hide}><span>{t("Instalando y reiniciando…")}</span></Notice>;
    case "error":
      return (
        <Notice className="gn-bar gn-err" role="alert" onClose={() => dispatch({ type: "close" })}>
          <span className="gn-dot">●</span>
          <span>{state.version ? t("No se pudo actualizar: {reason}", { reason: state.reason }) : t("No se pudo buscar actualizaciones: {reason}", { reason: state.reason })}</span>
          <span className="grow" />
          <Button variant="primary" size={28} onClick={() => dispatch({ type: "retry" })}>{t("Reintentar")}</Button>
        </Notice>
      );
    case "confirmSessions":
      return (
        <Notice className="gn-bar gn-wait" role="status" onClose={() => dispatch({ type: "cancel" })}>
          <span>{tn(state.running, "Hay {n} sesión en marcha; se detendrá al actualizar", "Hay {n} sesiones en marcha; se detendrán al actualizar")}</span>
          <span className="grow" />
          <Button variant="primary" size={28} onClick={() => dispatch({ type: "now" })}>{t("Actualizar ahora")}</Button>
          <Button variant="ghost" size={28} onClick={() => dispatch({ type: "whenIdle" })}>{t("Cuando terminen")}</Button>
          <Button variant="ghost" size={28} onClick={() => dispatch({ type: "cancel" })}>{t("Cancelar")}</Button>
        </Notice>
      );
    case "waiting":
      return (
        <Notice className="gn-bar gn-wait" role="status" onClose={hide}>
          <span>{tn(state.running, "Se actualizará cuando termine {n} sesión", "Se actualizará cuando terminen {n} sesiones")}</span>
          <span className="grow" />
          <Button variant="primary" size={28} onClick={() => dispatch({ type: "now" })}>{t("Actualizar ahora")}</Button>
          <Button variant="ghost" size={28} onClick={() => dispatch({ type: "cancel" })}>{t("Cancelar")}</Button>
        </Notice>
      );
  }
}
