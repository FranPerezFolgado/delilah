import { useEffect, useState } from "react";
import { api, ASK_POSITIONS, type AskPosition, type Health } from "../api";
import { isDesktop, revealInFinder } from "../platform";
import { Button, Row, Segmented, Toggle } from "./ui";
import "./SettingsView.css";
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

const SECTIONS = [
  { id: "general", label: "General" },
  { id: "preguntas", label: "Preguntas" },
  { id: "ahorro", label: "Modo ahorro" },
  { id: "actualizaciones", label: "Actualizaciones" },
  { id: "atajos", label: "Atajos de teclado" },
  { id: "diagnostico", label: "Diagnóstico" },
] as const;

/** Settings: a section index on the left and the sections in one column (max 720 px) on
 *  the right; only that column scrolls. */
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

  const [autoUpdate, setAutoUpdate] = useState(true);
  const [autoBusy, setAutoBusy] = useState(true);
  const [health, setHealth] = useState<Health | null>(null);
  const [current, setCurrent] = useState<string>(SECTIONS[0].id);

  useEffect(() => {
    api
      .preference<boolean>("auto-update")
      // On unless the user turned it off.
      .then((v) => setAutoUpdate(v !== false))
      .catch((e) => setError((e as Error).message))
      .finally(() => setAutoBusy(false));
    api.health().then(setHealth).catch(() => {});
  }, []);

  const toggleAuto = async (on: boolean) => {
    setError(null);
    setAutoBusy(true);
    try {
      await api.setPreference("auto-update", on);
      setAutoUpdate(on);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setAutoBusy(false);
    }
  };

  const goTo = (id: string) => {
    setCurrent(id);
    document.getElementById(`settings-${id}`)?.scrollIntoView?.({ block: "start" });
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
  const anyChanged = ACTIONS.some((a) => bindings[a.id] !== a.default);

  return (
    <>
      <div className="view-head" data-tauri-drag-region>
        <h1 data-tauri-drag-region>{t("Ajustes")}</h1>
      </div>
      <div className="settings">
        <nav className="settings-index" aria-label={t("Ajustes")}>
          {SECTIONS.map((s) => (
            <Row key={s.id} title={t(s.label)} current={current === s.id} onClick={() => goTo(s.id)} />
          ))}
        </nav>
        <div className="settings-main">
          <div className="settings-col">
            {error && <div className="error selectable">{error}</div>}
            <section id="settings-general" aria-labelledby="settings-general-h">
              <h2 id="settings-general-h">{t("General")}</h2>
              <div className="settings-field">
                <Segmented
                  mode="radio"
                  label={t("Idioma")}
                  value={language}
                  onChange={(l) => onLanguage(l as LangSetting)}
                  options={(["auto", "es", "en"] as LangSetting[]).map((l) => ({
                    value: l,
                    label: l === "auto" ? t("Automático (el del Mac)") : l === "es" ? "Español" : "English",
                  }))}
                />
                <p className="settings-help">
                  {t("También cambia lo que Delilah le escribe a Claude y las notificaciones. El menú del Mac cambia al volver a abrir la app.")}
                </p>
              </div>
              <div className="settings-field">
                <h3>{t("Control remoto")}</h3>
                <Toggle label={t("Abrir las sesiones de Claude con Remote Control")} pressed={remote} disabled={remoteBusy} onChange={toggleRemote} />
                <p className="settings-help">
                  {t("Podrás seguirlas y escribirles desde claude.ai o la app de Claude. Vale para las sesiones que se abran o reanuden a partir de ahora.")}
                </p>
                <p className="settings-help">
                  {t("Quien entre en tu cuenta de claude.ai podrá pedirles cambios en tu Mac. Necesita que Claude Code tenga la sesión iniciada con una cuenta de claude.ai, no con una clave de API.")}
                </p>
              </div>
            </section>
            <section id="settings-preguntas" aria-labelledby="settings-preguntas-h">
              <h2 id="settings-preguntas-h">{t("Preguntas")}</h2>
              <div className="settings-field">
                <Segmented
                  mode="radio"
                  label={t("Panel de preguntas")}
                  value={askPosition}
                  onChange={(p) => onAskPosition(p as AskPosition)}
                  options={ASK_POSITIONS.map((p) => ({
                    value: p,
                    label: p === "top" ? t("Arriba") : p === "right" ? t("A la derecha") : t("Abajo"),
                  }))}
                />
                <p className="settings-help">
                  {t("Dónde aparecen las preguntas con opciones del agente dentro de una sesión.")}
                </p>
              </div>
            </section>
            <section id="settings-ahorro" aria-labelledby="settings-ahorro-h">
              <h2 id="settings-ahorro-h">{t("Modo ahorro")}</h2>
              <div className="settings-field">
                <Toggle label={t("Modo ahorro por defecto")} pressed={saving} disabled={savingBusy} onChange={(on) => togglePref("saving-mode", on)} />
                <p className="settings-help">{t("Una sesión de Claude nueva por fase, para gastar menos contexto")}</p>
              </div>
              <div className="settings-field">
                <Toggle label={t("Guardar notas antes de cambiar de sesión")} pressed={saveNotes} disabled={savingBusy} onChange={(on) => togglePref("save-notes", on)} />
                <p className="settings-help">
                  {t("Antes de pasar a la sesión siguiente, la actual guarda sus notas con /save-session.")}
                </p>
              </div>
            </section>
            <section id="settings-actualizaciones" aria-labelledby="settings-actualizaciones-h">
              <h2 id="settings-actualizaciones-h">{t("Actualizaciones")}</h2>
              <div className="settings-field">
                <Toggle label={t("Buscar actualizaciones automáticamente")} pressed={autoUpdate} disabled={autoBusy} onChange={toggleAuto} />
                <p className="settings-help">{t("Si lo apagas, solo se busca cuando pulsas «Buscar ahora» o en el menú.")}</p>
              </div>
              {updates &&
                (updates.canUpdate ? (
                  <div className="settings-field">
                    <div className="settings-line">
                      <span>{t("Versión {version}", { version: updates.meta.installed })}</span>
                      <Button variant="secondary" onClick={() => void updates.check(true)}>{t("Buscar ahora")}</Button>
                    </div>
                    <p className="settings-help">{t("Última comprobación: {time}", { time: lastCheckText(updates.meta) })}</p>
                  </div>
                ) : (
                  <div className="settings-field">
                    <div>{updates.meta.installed && t("Versión {version}", { version: updates.meta.installed })}</div>
                    <p className="settings-help">{t("Las actualizaciones solo están en la app instalada")}</p>
                  </div>
                ))}
            </section>
            <section id="settings-atajos" aria-labelledby="settings-atajos-h">
              <div className="settings-line">
                <h2 id="settings-atajos-h">{t("Atajos de teclado")}</h2>
                <Button variant="ghost" className="settings-push" disabled={!anyChanged} onClick={() => save({ ...DEFAULTS })}>
                  {t("Restaurar todos")}
                </Button>
              </div>
              {groups.map((group) => (
                <div key={group} className="settings-group">
                  <h3>{t(group)}</h3>
                  <div className="settings-card">
                    {ACTIONS.filter((a) => a.group === group).map((a) => {
                      const other = clash(a.id);
                      const recording = recordingFor === a.id;
                      return (
                        <div key={a.id} className="settings-shortcut">
                          <span className="settings-sc-label">
                            {t(a.label)}
                            {other && <span className="settings-clash"> · {t("coincide con «{other}»", { other: t(other.label) })}</span>}
                          </span>
                          {recording ? (
                            <span className="settings-capture">{t("Pulsa la nueva combinación… · Esc para cancelar")}</span>
                          ) : (
                            <span className="kbd">{formatCombo(bindings[a.id])}</span>
                          )}
                          <Button variant={recording ? "primary" : "secondary"} size={28} onClick={() => setRecordingFor(recording ? null : a.id)}>
                            {recording ? t("Cancelar") : t("Cambiar")}
                          </Button>
                          {bindings[a.id] !== a.default && (
                            <Button variant="ghost" size={28} onClick={() => save({ ...bindings, [a.id]: a.default })}>
                              {t("Por defecto")}
                            </Button>
                          )}
                        </div>
                      );
                    })}
                  </div>
                </div>
              ))}
              <p className="settings-help">
                {t("Se guardan en Delilah, así que valen en todas las ventanas. Las combinaciones necesitan ⌘, ⌃ u ⌥ para no quitarle teclas a la terminal.")}
              </p>
            </section>
            <section id="settings-diagnostico" aria-labelledby="settings-diagnostico-h">
              <h2 id="settings-diagnostico-h">{t("Diagnóstico")}</h2>
              <div className="settings-field">
                <div className="settings-line">
                  <span>{t("Logs")}</span>
                  <Button variant="secondary" className="settings-push" onClick={() => window.dispatchEvent(new Event("delilah:show-logs"))}>
                    {t("Abrir")}
                  </Button>
                </div>
              </div>
              {health && (
                <>
                  <div className="settings-field">
                    <div className="settings-line">
                      <span>{t("Versión del daemon")}</span>
                      <span className="settings-value">{health.version}</span>
                    </div>
                  </div>
                  <div className="settings-field">
                    <div className="settings-line">
                      <span>{t("Carpeta de datos")}</span>
                      <span className="settings-value mono selectable">{health.home}</span>
                      {isDesktop && (
                        <Button variant="secondary" onClick={() => revealInFinder(health.home).catch((e) => setError((e as Error).message))}>
                          {t("Mostrar en Finder")}
                        </Button>
                      )}
                    </div>
                  </div>
                </>
              )}
            </section>
          </div>
        </div>
      </div>
    </>
  );
}
