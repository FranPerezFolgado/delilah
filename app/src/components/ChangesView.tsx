import { useCallback, useEffect, useMemo, useState } from "react";
import { api, type ChangeKind, type FileChange, type RepoChanges } from "../api";
import { Icon } from "./Icon";
import { t, tn } from "../i18n";

const KIND: Record<ChangeKind, { letter: string; label: string; cls: string }> = {
  added: { letter: "A", label: "Añadido", cls: "c-done" },
  untracked: { letter: "U", label: "Nuevo, sin añadir a git", cls: "c-done" },
  modified: { letter: "M", label: "Modificado", cls: "c-wait" },
  renamed: { letter: "R", label: "Renombrado", cls: "c-run" },
  deleted: { letter: "D", label: "Borrado", cls: "c-err" },
};

interface Props {
  /** A task's key, or `@<project>`. */
  owner: string;
  /** Opens a workspace file in a tab. */
  onEdit: (path: string) => void;
}

type Selected = { repo: string; path: string };

/** Everything the task's branch changed, per repo of the workspace, with each file's diff.
 *  Compared against where the branch started, uncommitted work included. */
export function ChangesView({ owner, onEdit }: Props) {
  const [repos, setRepos] = useState<RepoChanges[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Selected | null>(null);
  const [loading, setLoading] = useState(false);

  const load = useCallback(() => {
    setLoading(true);
    api
      .changes(owner)
      .then((r) => {
        setRepos(r);
        setError(null);
      })
      .catch((e) => setError((e as Error).message))
      .finally(() => setLoading(false));
  }, [owner]);

  useEffect(load, [load]);
  // The agent keeps working: coming back to the window refreshes the list.
  useEffect(() => {
    window.addEventListener("focus", load);
    return () => window.removeEventListener("focus", load);
  }, [load]);

  const flat = useMemo(() => (repos ?? []).flatMap((r) => r.files.map((f) => ({ repo: r.repo, file: f }))), [repos]);

  // Keep the selection while it still exists; else the first file.
  useEffect(() => {
    if (flat.length === 0) return setSelected(null);
    if (!selected || !flat.some((x) => x.repo === selected.repo && x.file.path === selected.path)) {
      setSelected({ repo: flat[0].repo, path: flat[0].file.path });
    }
  }, [flat, selected]);

  const move = (step: number) => {
    const at = flat.findIndex((x) => x.repo === selected?.repo && x.file.path === selected?.path);
    const next = flat[Math.min(flat.length - 1, Math.max(0, at + step))];
    if (next) setSelected({ repo: next.repo, path: next.file.path });
  };

  const totals = flat.reduce(
    (t, { file }) => ({ add: t.add + (file.additions ?? 0), del: t.del + (file.deletions ?? 0) }),
    { add: 0, del: 0 },
  );
  const current = flat.find((x) => x.repo === selected?.repo && x.file.path === selected?.path);
  const plainRepo = repos?.length === 1 && repos[0].repo === ".";

  return (
    <>
      <div className="ctoolbar">
        {repos && (
          <span className="muted" style={{ fontSize: 12 }}>
            {flat.length} {flat.length === 1 ? "archivo" : "archivos"} · <span className="c-done">+{totals.add}</span>{" "}
            <span className="c-err">−{totals.del}</span>
          </span>
        )}
        <button className="btn btn-ghost" style={{ marginLeft: "auto" }} onClick={load} disabled={loading}>
          <Icon name="refresh" /> {t("Refrescar")}
        </button>
      </div>
      {error && <div className="error selectable" style={{ padding: "12px 20px" }}>{error}</div>}
      {!repos && !error && (
        <div className="empty">
          <span>{t("Cargando…")}</span>
        </div>
      )}
      {repos && flat.length === 0 && (
        <div className="empty">
          <span>{t("La rama aún no ha cambiado nada.")}</span>
        </div>
      )}
      {flat.length > 0 && (
        <div className="changes">
          <div
            className="changes-files"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown" || e.key === "j") {
                e.preventDefault();
                move(1);
              } else if (e.key === "ArrowUp" || e.key === "k") {
                e.preventDefault();
                move(-1);
              }
            }}
          >
            {repos!.map((r) => (
              <div key={r.repo}>
                {!plainRepo && (
                  <div className="changes-repo">
                    <Icon name="folder" />
                    <b className="trunc">{r.repo}</b>
                    <span className="muted mono">{r.files.length}</span>
                  </div>
                )}
                {r.files.length > 0 && (
                  <div className="changes-meta muted">
                    {tn(r.commits, "{n} commit", "{n} commits")}
                    {r.base ? <> {t("desde")} <span className="mono">{r.base.slice(0, 8)}</span></> : t(" · sin base: solo lo no commiteado")}
                  </div>
                )}
                {r.files.map((f) => (
                  <FileRow
                    key={f.path}
                    file={f}
                    on={selected?.repo === r.repo && selected.path === f.path}
                    onClick={() => setSelected({ repo: r.repo, path: f.path })}
                  />
                ))}
              </div>
            ))}
          </div>
          <div className="changes-diff">
            {current && <Diff taskKey={owner} repo={current.repo} file={current.file} plainRepo={plainRepo} onEdit={onEdit} />}
          </div>
        </div>
      )}
    </>
  );
}

function FileRow({ file, on, onClick }: { file: FileChange; on: boolean; onClick: () => void }) {
  const kind = KIND[file.kind];
  const slash = file.path.lastIndexOf("/");
  return (
    <button className={`frow ${on ? "on" : ""}`} onClick={onClick} title={file.path}>
      <span className={`kind mono ${kind.cls}`} title={t(kind.label)}>
        {kind.letter}
      </span>
      <span className="trunc" style={{ flex: 1 }}>
        {file.path.slice(slash + 1)}
        {slash > 0 && <span className="muted"> {file.path.slice(0, slash)}</span>}
      </span>
      {file.additions === null ? (
        <span className="muted mono">bin</span>
      ) : (
        <span className="mono counts">
          {file.additions > 0 && <span className="c-done">+{file.additions}</span>}
          {file.deletions! > 0 && <span className="c-err">−{file.deletions}</span>}
        </span>
      )}
    </button>
  );
}

type Line = { type: "add" | "del" | "ctx" | "hunk" | "note"; text: string; old?: number; new?: number };

/** Unified diff to rows with old and new line numbers. Git's own headers are dropped. */
function parse(diff: string): Line[] {
  const lines: Line[] = [];
  let o = 0;
  let n = 0;
  let inHunk = false;
  for (const raw of diff.split("\n")) {
    const hunk = /^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@(.*)/.exec(raw);
    if (hunk) {
      o = Number(hunk[1]);
      n = Number(hunk[2]);
      inHunk = true;
      lines.push({ type: "hunk", text: raw });
    } else if (!inHunk) {
      if (raw.startsWith("Binary files") || raw === "Binary file") lines.push({ type: "note", text: "Archivo binario" });
    } else if (raw.startsWith("+")) {
      lines.push({ type: "add", text: raw.slice(1), new: n++ });
    } else if (raw.startsWith("-")) {
      lines.push({ type: "del", text: raw.slice(1), old: o++ });
    } else if (raw.startsWith("\\")) {
      lines.push({ type: "note", text: raw.slice(2) });
    } else if (raw.startsWith(" ")) {
      lines.push({ type: "ctx", text: raw.slice(1), old: o++, new: n++ });
    } else if (raw.startsWith("diff --git")) {
      inHunk = false;
    }
  }
  return lines;
}

function Diff({
  taskKey,
  repo,
  file,
  plainRepo,
  onEdit,
}: {
  taskKey: string;
  repo: string;
  file: FileChange;
  plainRepo: boolean;
  onEdit: (path: string) => void;
}) {
  const [text, setText] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    setText(null);
    setError(null);
    api
      .diff(taskKey, repo, file.path)
      .then((t) => alive && setText(t))
      .catch((e) => alive && setError((e as Error).message));
    return () => {
      alive = false;
    };
    // Counts change when the agent edits the file: fetch the diff again.
  }, [taskKey, repo, file.path, file.additions, file.deletions]);

  const lines = useMemo(() => (text ? parse(text) : []), [text]);
  const where = plainRepo ? file.path : `${repo}/${file.path}`;

  return (
    <>
      <div className="diff-head">
        <span className="mono selectable trunc">
          {file.old_path && <span className="muted">{file.old_path} → </span>}
          {where}
        </span>
        <span className="muted" style={{ marginLeft: "auto", flex: "none" }}>{t(KIND[file.kind].label)}</span>
        {file.kind !== "deleted" && (
          <button className="btn btn-ghost" style={{ flex: "none" }} onClick={() => onEdit(where)}>
            <Icon name="code" /> {t("Abrir")}
          </button>
        )}
      </div>
      {error && <div className="error" style={{ padding: 12 }}>{error}</div>}
      {text !== null && lines.length === 0 && <div className="muted" style={{ padding: 12 }}>{t("Sin cambios de contenido.")}</div>}
      <div className="diff selectable">
        {lines.map((l, i) =>
          l.type === "hunk" ? (
            <div key={i} className="dl hunk">
              <span className="ln" />
              <span className="ln" />
              <span className="code">{l.text}</span>
            </div>
          ) : l.type === "note" ? (
            <div key={i} className="dl note">
              <span className="ln" />
              <span className="ln" />
              <span className="code">{l.text}</span>
            </div>
          ) : (
            <div key={i} className={`dl ${l.type}`}>
              <span className="ln">{l.old ?? ""}</span>
              <span className="ln">{l.new ?? ""}</span>
              <span className="code">
                <span className="sign">{l.type === "add" ? "+" : l.type === "del" ? "−" : " "}</span>
                {l.text}
              </span>
            </div>
          ),
        )}
      </div>
    </>
  );
}
