import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { api, type ChangeKind, type TreeEntry } from "../api";

const MARK: Partial<Record<ChangeKind, string>> = {
  modified: "c-wait",
  renamed: "c-run",
  added: "c-done",
  untracked: "c-done",
};

interface Props {
  taskKey: string;
  /** Changed files by workspace path, to mark them. */
  changed: Map<string, ChangeKind>;
  active: string | null;
  expanded: Set<string>;
  /** Bumped to reload the open folders. */
  refresh: number;
  onOpen: (path: string) => void;
}

/** The workspace's folders, loaded one at a time as they are opened. */
export function FileTree({ taskKey, changed, active, expanded, refresh, onOpen }: Props) {
  const [children, setChildren] = useState<Map<string, TreeEntry[]>>(new Map());
  const [, redraw] = useState(0);

  const load = useCallback(
    (dir: string) =>
      api
        .tree(taskKey, dir)
        .then((entries) => setChildren((m) => new Map(m).set(dir, entries)))
        .catch(() => {}),
    [taskKey],
  );

  // The root and every open folder, again whenever asked to refresh.
  useEffect(() => {
    load("");
    expanded.forEach((dir) => load(dir));
  }, [load, expanded, refresh]);

  // Folders opened from outside (revealing a file) load as soon as they show up.
  const pending = useRef(new Set<string>());
  useEffect(() => {
    expanded.forEach((dir) => {
      if (children.has(dir) || pending.current.has(dir)) return;
      pending.current.add(dir);
      load(dir).finally(() => pending.current.delete(dir));
    });
  });

  // Folders that hold a change get a dot, so changes are easy to find.
  const changedDirs = useMemo(() => {
    const dirs = new Set<string>();
    for (const path of changed.keys()) {
      for (let i = path.indexOf("/"); i > 0; i = path.indexOf("/", i + 1)) dirs.add(path.slice(0, i));
    }
    return dirs;
  }, [changed]);

  const toggle = (dir: string) => {
    if (expanded.has(dir)) expanded.delete(dir);
    else {
      expanded.add(dir);
      if (!children.has(dir)) load(dir);
    }
    redraw((n) => n + 1);
  };

  const rows = (dir: string, depth: number): ReactNode[] =>
    (children.get(dir) ?? []).flatMap((e) => {
      const path = dir ? `${dir}/${e.name}` : e.name;
      const pad = { paddingLeft: 8 + depth * 12 };
      if (e.dir) {
        const open = expanded.has(path);
        return [
          <button key={path} className="trow" style={pad} onClick={() => toggle(path)} title={path}>
            <span className={`chev ${open ? "open" : ""}`}>›</span>
            <span className="trunc" style={{ flex: 1 }}>{e.name}</span>
            {changedDirs.has(path) && <span className="tdot" />}
          </button>,
          ...(open ? rows(path, depth + 1) : []),
        ];
      }
      const kind = changed.get(path);
      return [
        <button
          key={path}
          className={`trow ${active === path ? "on" : ""} ${kind ? MARK[kind] ?? "" : ""}`}
          style={pad}
          onClick={() => onOpen(path)}
          title={path}
        >
          <span className="chev" />
          <span className="trunc" style={{ flex: 1 }}>{e.name}</span>
          {kind && <span className="mono tkind">{kind === "untracked" ? "U" : kind[0].toUpperCase()}</span>}
        </button>,
      ];
    });

  return (
    <div className="tree">
      {children.has("") ? rows("", 0) : <div className="muted" style={{ padding: 10 }}>Cargando…</div>}
      {children.get("")?.length === 0 && <div className="muted" style={{ padding: 10 }}>El workspace está vacío.</div>}
    </div>
  );
}
