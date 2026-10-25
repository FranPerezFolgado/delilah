import { useCallback, useEffect, useMemo, useState } from "react";
import { api, type ChangeKind, type FileChange, type RepoChanges } from "../api";
import { Icon } from "./Icon";
import { Button, Skeleton } from "./ui";
import "./ChangesView.css";
import { DiffRows, type DiffRowData } from "./DiffRows";
import { t, tn } from "../i18n";

const KIND: Record<ChangeKind, { letter: string; label: string; cls: string }> = {
  added: { letter: "A", label: "Añadido", cls: "c-done" },
  untracked: { letter: "A", label: "Nuevo, sin añadir a git", cls: "c-done" },
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
type Entry = { repo: string; file: FileChange };
interface Folder { dir: string; files: FileChange[] }

/** The files of a repo by folder (root first, then alphabetical), keeping those that match `filter`. */
function groupByFolder(files: FileChange[], filter: string): Folder[] {
  const q = filter.trim().toLowerCase();
  const by = new Map<string, FileChange[]>();
  for (const f of files) {
    if (q && !f.path.toLowerCase().includes(q)) continue;
    const dir = f.path.includes("/") ? f.path.slice(0, f.path.lastIndexOf("/")) : "";
    by.set(dir, [...(by.get(dir) ?? []), f]);
  }
  return [...by.entries()]
    .sort(([a], [b]) => (a === "" ? -1 : b === "" ? 1 : a.localeCompare(b)))
    .map(([dir, fs]) => ({ dir, files: fs.sort((x, y) => x.path.localeCompare(y.path)) }));
}

/** Everything the task's branch changed, per repo of the workspace, with each file's diff.
 *  Compared against where the branch started, uncommitted work included. */
export function ChangesView({ owner, onEdit }: Props) {
  const [repos, setRepos] = useState<RepoChanges[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Selected | null>(null);
  const [loading, setLoading] = useState(false);
  const [filter, setFilter] = useState("");

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

  // Per repo, the files that match the filter by folder; `flat` follows the order they are drawn in.
  const shown = useMemo(
    () => (repos ?? []).map((r) => ({ r, folders: groupByFolder(r.files, filter) })),
    [repos, filter],
  );
  const flat: Entry[] = useMemo(
    () => shown.flatMap(({ r, folders }) => folders.flatMap((d) => d.files.map((file) => ({ repo: r.repo, file })))),
    [shown],
  );
  const total = (repos ?? []).reduce((n, r) => n + r.files.length, 0);

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
          <span className="muted changes-summary">
            {tn(flat.length, "{n} archivo", "{n} archivos")} · <span className="c-done">+{totals.add}</span>{" "}
            <span className="c-err">−{totals.del}</span>
          </span>
        )}
        <input
          className="input changes-filter"
          type="search"
          value={filter}
          placeholder={t("Filtrar archivos…")}
          aria-label={t("Filtrar archivos")}
          onChange={(e) => setFilter(e.target.value)}
        />
        <Button variant="ghost" size={28} icon="refresh" onClick={load} disabled={loading}>
          {t("Recargar")}
        </Button>
      </div>
      {error && <div className="error selectable changes-error">{error}</div>}
      {!repos && !error && (
        <div className="changes-loading" role="status" aria-label={t("Cargando…")}>
          <Skeleton lines={6} />
        </div>
      )}
      {repos && total === 0 && (
        <div className="empty">
          <span>{t("La rama aún no ha cambiado nada.")}</span>
        </div>
      )}
      {repos && total > 0 && flat.length === 0 && (
        <div className="empty">
          <span>{t("Ningún archivo coincide con el filtro.")}</span>
        </div>
      )}
      {flat.length > 0 && (
        <div className="changes">
          <div
            className="changes-files"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.target instanceof HTMLInputElement) return;
              if (e.key === "ArrowDown" || e.key === "j") {
                e.preventDefault();
                move(1);
              } else if (e.key === "ArrowUp" || e.key === "k") {
                e.preventDefault();
                move(-1);
              }
            }}
          >
            {shown.map(({ r, folders }) => (
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
                {folders.map((d) => (
                  <div key={d.dir} className="changes-folder" role="group" aria-label={d.dir || t("Raíz")}>
                    {(d.dir !== "" || folders.length > 1) && (
                      <div className="changes-dir muted">
                        <Icon name="folder" />
                        <span className="trunc mono">{d.dir || "/"}</span>
                        <span className="mono">{d.files.length}</span>
                      </div>
                    )}
                    {d.files.map((f) => (
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
      <span className="trunc frow-name">
        {file.path.slice(slash + 1)}
        {file.old_path && <span className="muted"> ← {file.old_path}</span>}
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

type Line = DiffRowData;

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
      lines.push({ kind: "hunk", text: raw, old: null, new: null });
    } else if (!inHunk) {
      if (raw.startsWith("Binary files") || raw === "Binary file") lines.push({ kind: "note", text: "Archivo binario", old: null, new: null });
    } else if (raw.startsWith("+")) {
      lines.push({ kind: "add", text: raw.slice(1), old: null, new: n++ });
    } else if (raw.startsWith("-")) {
      lines.push({ kind: "del", text: raw.slice(1), old: o++, new: null });
    } else if (raw.startsWith("\\")) {
      lines.push({ kind: "note", text: raw.slice(2), old: null, new: null });
    } else if (raw.startsWith(" ")) {
      lines.push({ kind: "ctx", text: raw.slice(1), old: o++, new: n++ });
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
        <span className="muted diff-kind">{t(KIND[file.kind].label)}</span>
        {file.kind !== "deleted" && (
          <Button variant="ghost" size={28} icon="code" onClick={() => onEdit(where)}>
            {t("Abrir")}
          </Button>
        )}
      </div>
      {error && <div className="error changes-error">{error}</div>}
      {text !== null && lines.length === 0 && <div className="muted changes-error">{t("Sin cambios de contenido.")}</div>}
      <div className="diff selectable">
        <DiffRows rows={lines} />
      </div>
    </>
  );
}
