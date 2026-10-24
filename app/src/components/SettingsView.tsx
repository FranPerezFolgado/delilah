import { useEffect, useState } from "react";
import { api, ASK_POSITIONS, type AskPosition } from "../api";
import { locale, t, type LangSetting } from "../i18n";
import type { UpdateMeta } from "../updates";
import { ACTIONS, type ActionId, type Bindings, comboFromEvent, DEFAULTS, formatCombo, setRecording } from "../shortcuts";

interface Props {
  bindings: Bindings;
  onChange: (b: Bindings) => void;
  language: LangSetting;
  onLanguage: (l: LangSetting) => void;
  askPosition: AskPosition;
  onAskPosition: (p: AskPosition) => void;
  /** The update flow's data; the section is left out when absent. */
  updates?: { meta: UpdateMeta; canUpdate: boolean; check: (manual: boolean) => void | Promise<void> };
}

/** "when · result" of the last update check, or "Todavía no". */
function lastCheckText(meta: UpdateMeta): string {
  if (!meta.lastCheck) return t("Todavía no");
  const when = meta.lastCheck.toLocaleString(locale(), { day: "numeric", month: "short", hour: "2-digit", minute: "2-digit" });
  const result =
    meta.lastResult === "upToDate" ? t("Delilah está al día")
    : meta.lastResult === "available" ? t("Hay una versión nueva")
    : meta.lastResult === "error" ? t("falló")
    : "";
  return result ? `${when} · ${result}` : when;
}

/** Settings: the language, where the agent's questions sit, Remote Control, and the
 *  keyboard shortcuts (press "Cambiar", then the new keys). */
export function SettingsView({ bindings, onChange, language, onLanguage, askPosition, onAskPosition, updates }: Props) {
  const [recordingFor, setRecordingFor] = useState<ActionId | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [remote, setRemote] = useState(false);
  // Off until the saved value is in, so a click can never race the first load.
  const [remoteBusy, setRemoteBusy] = useState(true);

  useEffect(() => {
    api
      .preference<boolean>("remote-control")
      .then((v) => setRemote(v === true))
      .catch((e) => setError((e as Error).message))
      .finally(() => setRemoteBusy(false));
  }, []);

  const toggleRemote = async (on: boolean) => {
    setError(null);
    setRemoteBusy(true);
    try {
      await api.setPreference("remote-control", on);
      setRemote(on);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setRemoteBusy(false);
    }
  };

  const [saving, setSaving] = useState(false);
  const [saveNotes, setSaveNotes] = useState(true);
  const [savingBusy, setSavingBusy] = useState(true);

  useEffect(() => {
    Promise.all([api.preference<boolean>("saving-mode"), api.preference<boolean>("save-notes")])
      .then(([mode, notes]) => {
        setSaving(mode === true);
        // On unless the user turned it off.
        setSaveNotes(notes !== false);
      })
      .catch((e) => setError((e as Error).message))
      .finally(() => setSavingBusy(false));
  }, []);

  const togglePref = async (name: "saving-mode" | "save-notes", on: boolean) => {
    setError(null);
    setSavingBusy(true);
    try {
      await api.setPreference(name, on);
      (name === "saving-mode" ? setSaving : setSaveNotes)(on);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSavingBusy(false);
    }
  };

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
        <div>
          <div className="sec" style={{ marginBottom: 8 }}>{t("Remote Control")}</div>
          <label style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
            <input
              type="checkbox"
              checked={remote}
              disabled={remoteBusy}
              onChange={(e) => toggleRemote(e.target.checked)}
            />
            {t("Abrir las sesiones de Claude con Remote Control")}
          </label>
          <p className="muted" style={{ margin: "6px 0 0", fontSize: 12 }}>
            {t("Podrás seguirlas y escribirles desde claude.ai o la app de Claude. Vale para las sesiones que se abran o reanuden a partir de ahora.")}
          </p>
          <p className="muted" style={{ margin: "6px 0 0", fontSize: 12 }}>
            {t("Quien entre en tu cuenta de claude.ai podrá pedirles cambios en tu Mac. Necesita que Claude Code tenga la sesión iniciada con una cuenta de claude.ai, no con una clave de API.")}
          </p>
        </div>
        <div>
          <div className="sec" style={{ marginBottom: 8 }}>{t("Modo ahorro")}</div>
          <label style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
            <input
              type="checkbox"
              checked={saving}
              disabled={savingBusy}
              onChange={(e) => togglePref("saving-mode", e.target.checked)}
            />
            {t("Modo ahorro por defecto")}
          </label>
          <p className="muted" style={{ margin: "6px 0 0", fontSize: 12 }}>
            {t("Las tareas nuevas usan una sesión de Claude por fase, para gastar menos contexto.")}
          </p>
          <label style={{ display: "inline-flex", alignItems: "center", gap: 6, marginTop: 8 }}>
            <input
              type="checkbox"
              checked={saveNotes}
              disabled={savingBusy}
              onChange={(e) => togglePref("save-notes", e.target.checked)}
            />
            {t("Guardar notas antes de cambiar de sesión")}
          </label>
          <p className="muted" style={{ margin: "6px 0 0", fontSize: 12 }}>
            {t("Antes de pasar a la sesión siguiente, la actual guarda sus notas con /save-session.")}
          </p>
        </div>
        {updates && (
          <div>
            <div className="sec" style={{ marginBottom: 8 }}>{t("Actualizaciones")}</div>
            {updates.canUpdate ? (
              <>
                <div style={{ display: "flex", alignItems: "center", gap: 10 }}>
                  <span>{t("Versión {version}", { version: updates.meta.installed })}</span>
                  <button className="btn" onClick={() => void updates.check(true)}>{t("Buscar ahora")}</button>
                </div>
                <p className="muted" style={{ margin: "6px 0 0", fontSize: 12 }}>
                  {t("Última comprobación: {time}", { time: lastCheckText(updates.meta) })}
                </p>
              </>
            ) : (
              <>
                <div>{updates.meta.installed && t("Versión {version}", { version: updates.meta.installed })}</div>
                <p className="muted" style={{ margin: "6px 0 0", fontSize: 12 }}>
                  {t("Las actualizaciones solo están en la app instalada")}
                </p>
              </>
            )}
          </div>
        )}
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
