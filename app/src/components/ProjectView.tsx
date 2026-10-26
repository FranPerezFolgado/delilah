import { useEffect, useState } from "react";
import { api, type Project, type Session, type SessionKind, type Task, type TreeEntry } from "../api";
import { keyOf } from "../shortcuts";
import { relativeTime } from "../time";
import { Icon } from "./Icon";
import { Markdown } from "./Markdown";
import { kindLabel, sessionStatus } from "./sessionLabels";
import { Button, Row } from "./ui";
import { t } from "../i18n";
import "./ProjectView.css";

interface Props {
  project: Project;
  sessions: Session[];
  /** All tasks; the project's active ones are listed. */
  tasks?: Task[];
  onOpenSession: (id?: string) => void;
  onNewSession: (kind: SessionKind) => Promise<void>;
  onNewTask?: () => void;
  /** Opens one of the project's tasks. */
  onOpenTask?: (key: string) => void;
  /** "Nueva tarea con Claude": an idea conversation. */
  onNewIdea?: () => void;
}

/** A project's first tab: its checkout, and sessions to ask about the code without a task. */
export function ProjectView({ project, sessions, tasks = [], onOpenSession, onNewSession, onNewTask, onOpenTask, onNewIdea }: Props) {
  const saved = project.preview_command ?? "";
  const [command, setCommand] = useState(saved);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => setCommand(saved), [saved]);
  const dirty = command.trim() !== saved;
  const saveCommand = async () => {
    const next = command.trim();
    if (next === saved) return;
    setError(null);
    try {
      await api.updateProject(project.name, { preview_command: next === "" ? null : next });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const active = tasks.filter((tk) => tk.project === project.name && tk.status.type !== "done" && tk.status.type !== "queued");
  const conversations = sessions.filter((s) => !s.conversation);
  const ideas = sessions.filter((s) => s.conversation);

  const sessionRow = (s: Session) => {
    const st = sessionStatus(s, undefined);
    return (
      <Row
        key={s.id}
        leading={<Icon name={s.kind !== "claude" ? "console" : st.icon} className={st.cls} />}
        title={s.label}
        sub={s.last_message ?? kindLabel(s, s.read_only ? t("Claude · solo lectura") : "Claude")}
        trailing={s.last_activity_at ? <span className="small">{relativeTime(s.last_activity_at)}</span> : undefined}
        onClick={() => onOpenSession(s.id)}
      />
    );
  };

  return (
    <div className="view-body prj">
      <header className="prj-head">
        <Icon name="folder" />
        <h2 className="prj-name">{project.name}</h2>
        <span className="mono muted selectable prj-path">{project.path}</span>
      </header>
      <div className="prj-actions">
        <Button variant="primary" icon="plus" onClick={() => onNewSession("claude")} title={t("Arranca en solo lectura sobre tu checkout: lee, busca y explica, pero no edita hasta que se lo permitas.")}>
          {t("Preguntar a Claude")} <span className="kbd">{keyOf("new-claude")}</span>
        </Button>
        {onNewTask && (
          <Button variant="secondary" icon="tasks" onClick={onNewTask}>
            {t("Nueva tarea")} <span className="kbd">{keyOf("new-task")}</span>
          </Button>
        )}
        <Button variant="secondary" icon="plus" onClick={() => onNewSession("shell")}>
          {t("Terminal")} <span className="kbd">{keyOf("new-terminal")}</span>
        </Button>
        {onNewIdea && (
          <Button variant="ghost" icon="idea" onClick={onNewIdea}>{t("Nueva tarea con Claude")}</Button>
        )}
      </div>

      <section className="prj-sec" aria-label={t("Conversaciones")}>
        <div className="sec">{t("Conversaciones")}</div>
        {conversations.length === 0 ? <p className="muted small">{t("Aún no hay conversaciones.")}</p> : conversations.map(sessionRow)}
      </section>
      <section className="prj-sec" aria-label={t("Ideas")}>
        <div className="sec">{t("Ideas")}</div>
        {ideas.length === 0 ? <p className="muted small">{t("Aún no hay ideas.")}</p> : ideas.map(sessionRow)}
      </section>
      <section className="prj-sec" aria-label={t("Tareas activas")}>
        <div className="sec">{t("Tareas activas")}</div>
        {active.length === 0 ? (
          <p className="muted small">{t("Ninguna tarea activa.")}</p>
        ) : (
          active.map((tk) => (
            <Row key={tk.key} title={tk.title} sub={tk.key} onClick={() => onOpenTask?.(tk.key)} />
          ))
        )}
      </section>

      <section className="prj-sec">
        <label className="field" htmlFor="prj-preview">
          <span>{t("Comando de preview")}</span>
        </label>
        <div className="prj-cmd">
          <input
            id="prj-preview"
            className="input mono"
            value={command}
            aria-describedby="prj-preview-help"
            onChange={(e) => setCommand(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && saveCommand()}
          />
          <Button variant="secondary" disabled={!dirty} onClick={saveCommand}>{t("Guardar")}</Button>
        </div>
        <p id="prj-preview-help" className="muted small">
          {t("Delilah lo ejecuta en el workspace de una tarea para que puedas probarla. Déjalo vacío si el proyecto no tiene preview.")}
        </p>
        {error && <p className="error selectable">{error}</p>}
      </section>

      <Readme owner={`@${project.name}`} />
      <Tree owner={`@${project.name}`} />
    </div>
  );
}

function Readme({ owner }: { owner: string }) {
  const [text, setText] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    api.file(owner, "README.md").then((f) => live && setText(f.text)).catch(() => live && setText(null));
    return () => {
      live = false;
    };
  }, [owner]);
  if (!text) return null;
  return (
    <section className="prj-sec" aria-label="README">
      <div className="sec">README</div>
      <div className="card prj-readme"><Markdown text={text} remoteImages="click" /></div>
    </section>
  );
}

function Tree({ owner }: { owner: string }) {
  const [entries, setEntries] = useState<TreeEntry[] | null>(null);
  useEffect(() => {
    let live = true;
    api.tree(owner, "").then((e) => live && setEntries(e)).catch(() => live && setEntries(null));
    return () => {
      live = false;
    };
  }, [owner]);
  if (!entries || entries.length === 0) return null;
  return (
    <section className="prj-sec" aria-label={t("Archivos")}>
      <div className="sec">{t("Archivos")}</div>
      <ul className="prj-tree">
        {entries.map((e) => <TreeNode key={e.name} owner={owner} path={e.name} entry={e} />)}
      </ul>
    </section>
  );
}

function TreeNode({ owner, path, entry }: { owner: string; path: string; entry: TreeEntry }) {
  const [open, setOpen] = useState(false);
  const [children, setChildren] = useState<TreeEntry[] | null>(null);
  const toggle = () => {
    if (!open && children === null) api.tree(owner, path).then(setChildren).catch(() => setChildren([]));
    setOpen(!open);
  };
  return (
    <li>
      {entry.dir ? (
        <button type="button" className="prj-node" aria-expanded={open} onClick={toggle}>
          <Icon name="folder" /> {entry.name}
        </button>
      ) : (
        <span className="prj-node prj-file">{entry.name}</span>
      )}
      {open && children && (
        <ul className="prj-tree">
          {children.map((c) => <TreeNode key={c.name} owner={owner} path={`${path}/${c.name}`} entry={c} />)}
        </ul>
      )}
    </li>
  );
}
