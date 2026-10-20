import { useEffect, useState } from "react";
import { api, ASK_POSITIONS, type AskPosition } from "../api";
import { t, type LangSetting } from "../i18n";
import { ACTIONS, type ActionId, type Bindings, comboFromEvent, DEFAULTS, formatCombo, setRecording } from "../shortcuts";

interface Props {
  bindings: Bindings;
  onChange: (b: Bindings) => void;
  language: LangSetting;
  onLanguage: (l: LangSetting) => void;
  askPosition: AskPosition;
  onAskPosition: (p: AskPosition) => void;
}

/** Settings: the language, where the agent's questions sit, and the keyboard shortcuts
 *  (press "Cambiar", then the new keys). */
export function SettingsView({ bindings, onChange, language, onLanguage, askPosition, onAskPosition }: Props) {
  const [recordingFor, setRecordingFor] = useState<ActionId | null>(null);
  const [error, setError] = useState<string | null>(null);

  const save = async (next: Bindings) => {
    setError(null);
    try {
      // Only what differs from the defaults: a later change of default still applies.
      const changed = Object.fromEntries(Object.entries(next).filter(([id, c]) => DEFAULTS[id as ActionId] !== c));
      await api.setPreference("shortcuts", changed);
      onChange(next);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  useEffect(() => {
    if (!recordingFor) return;
    setRecording(true);
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      if (e.key === "Escape") return setRecordingFor(null);
      const combo = comboFromEvent(e);
      if (!combo) return; // A bare key: keep waiting for one with a modifier.
      save({ ...bindings, [recordingFor]: combo });
      setRecordingFor(null);
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      setRecording(false);
    };
  }, [recordingFor]);

  const clash = (id: ActionId) => ACTIONS.find((a) => a.id !== id && bindings[a.id] === bindings[id]);
  const groups = [...new Set(ACTIONS.map((a) => a.group))];

  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        <h1 data-tauri-drag-region>{t("Ajustes")}</h1>
      </div>
      <div className="view-body">
        <div>
          <div className="sec" style={{ marginBottom: 8 }}>{t("Idioma")}</div>
          <div className="segs" role="radiogroup" aria-label={t("Idioma")} style={{ display: "inline-flex" }}>
            {(["auto", "es", "en"] as LangSetting[]).map((l) => (
              <button key={l} role="radio" aria-checked={language === l} className={`seg ${language === l ? "on" : ""}`} onClick={() => onLanguage(l)}>
                {l === "auto" ? t("Automático (el del Mac)") : l === "es" ? "Español" : "English"}
              </button>
            ))}
          </div>
          <p className="muted" style={{ margin: "6px 0 0", fontSize: 12 }}>
            {t("También cambia lo que Delilah le escribe a Claude y las notificaciones. El menú del Mac cambia al volver a abrir la app.")}
          </p>
        </div>
        <div>
          <div className="sec" style={{ marginBottom: 8 }}>{t("Panel de preguntas")}</div>
          <div className="segs" role="radiogroup" aria-label={t("Panel de preguntas")} style={{ display: "inline-flex" }}>
            {ASK_POSITIONS.map((p) => (
              <button key={p} role="radio" aria-checked={askPosition === p} className={`seg ${askPosition === p ? "on" : ""}`} onClick={() => onAskPosition(p)}>
                {p === "top" ? t("Arriba") : p === "right" ? t("A la derecha") : t("Abajo")}
              </button>
            ))}
          </div>
          <p className="muted" style={{ margin: "6px 0 0", fontSize: 12 }}>
            {t("Dónde aparecen las preguntas con opciones del agente dentro de una sesión.")}
          </p>
        </div>
        <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
          <span className="sec">{t("Atajos de teclado")}</span>
          <button className="btn" style={{ marginLeft: "auto" }} onClick={() => save({ ...DEFAULTS })}>
            {t("Restablecer todos")}
          </button>
        </div>
        {error && <div className="error selectable">{error}</div>}
        {groups.map((group) => (
          <div key={group}>
            <div className="muted" style={{ fontSize: 12, margin: "4px 0 6px" }}>{t(group)}</div>
            <div className="card history">
              {ACTIONS.filter((a) => a.group === group).map((a) => {
                const other = clash(a.id);
                const recording = recordingFor === a.id;
                return (
                  <div key={a.id} className="row" style={{ gridTemplateColumns: "1fr auto auto auto", alignItems: "center", gap: 10 }}>
                    <span>
                      {t(a.label)}
                      {other && <span className="c-err" style={{ fontSize: 12 }}> · {t("coincide con «{other}»", { other: t(other.label) })}</span>}
                    </span>
                    <span className="kbd" style={{ fontSize: 12, padding: "2px 6px", minWidth: 44, textAlign: "center" }}>
                      {recording ? "…" : formatCombo(bindings[a.id])}
                    </span>
                    <button className={`btn ${recording ? "btn-p" : ""}`} onClick={() => setRecordingFor(recording ? null : a.id)}>
                      {recording ? t("Pulsa las teclas (Esc cancela)") : t("Cambiar")}
                    </button>
                    <button className="btn btn-ghost" disabled={bindings[a.id] === a.default}
                      onClick={() => save({ ...bindings, [a.id]: a.default })}>
                      {t("Por defecto")}
                    </button>
                  </div>
                );
              })}
            </div>
          </div>
        ))}
        <p className="muted" style={{ margin: 0, fontSize: 12 }}>
          {t("Se guardan en Delilah, así que valen en todas las ventanas. Las combinaciones necesitan ⌘, ⌃ u ⌥ para no quitarle teclas a la terminal.")}
        </p>
      </div>
    </>
  );
}
