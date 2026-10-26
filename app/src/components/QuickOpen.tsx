import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "../api";
import { t, tn } from "../i18n";
import { Dialog } from "./ui/Dialog";
import "./QuickOpen.css";

const SHOWN = 50;

/** Subsequence score of `q` in `text` (both lower case), or null if it does not fit.
 *  Letters in a row and at word starts count more. */
function fit(q: string, text: string): number | null {
  let total = 0;
  let at = -1;
  let run = 0;
  for (const ch of q) {
    const found = text.indexOf(ch, at + 1);
    if (found < 0) return null;
    run = found === at + 1 ? run + 1 : 0;
    total += 1 + run * 2;
    const prev = text[found - 1];
    if (found === 0 || prev === "/" || prev === "-" || prev === "_" || prev === ".") total += 3;
    at = found;
  }
  return total;
}

/** How well `query` matches `path`, or null if it does not. The file name comes
 *  first: starting with the query, then containing it, then fitting it; else the
 *  whole path. Shorter paths win ties. */
export function score(query: string, path: string): number | null {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return 0;
  const p = path.toLowerCase();
  const name = p.slice(p.lastIndexOf("/") + 1);
  const tie = -path.length * 0.01;
  if (name.startsWith(q)) return 1000 + tie;
  if (name.includes(q)) return 800 + tie;
  const inName = fit(q, name);
  if (inName !== null) return 400 + inName + tie;
  const inPath = fit(q, p);
  return inPath === null ? null : inPath + tie;
}

/** Where `q` fits in `text` as a subsequence (indices), or null. */
function fitAt(q: string, text: string): number[] | null {
  const out: number[] = [];
  let at = -1;
  for (const ch of q) {
    at = text.indexOf(ch, at + 1);
    if (at < 0) return null;
    out.push(at);
  }
  return out;
}

/** Indices of `path` to show in bold for `query`: the characters `score` matched,
 *  in the same order of preference (file name first, then the whole path). */
export function matchedAt(query: string, path: string): Set<number> {
  const q = query.toLowerCase().replace(/\s+/g, "");
  if (!q) return new Set();
  const p = path.toLowerCase();
  const base = p.lastIndexOf("/") + 1;
  const name = p.slice(base);
  const contiguous = name.startsWith(q) ? 0 : name.indexOf(q);
  if (contiguous >= 0) return new Set(Array.from({ length: q.length }, (_, i) => base + contiguous + i));
  const inName = fitAt(q, name);
  if (inName) return new Set(inName.map((i) => base + i));
  return new Set(fitAt(q, p) ?? []);
}

function Marked({ text, from, on }: { text: string; from: number; on: Set<number> }) {
  return (
    <>
      {[...text].map((ch, i) => (on.has(from + i) ? <b key={i}>{ch}</b> : ch))}
    </>
  );
}

interface Props {
  taskKey: string;
  onOpen: (path: string) => void;
  onClose: () => void;
}

/** ⌘P: find a file of the workspace by typing part of its name. */
export function QuickOpen({ taskKey, onOpen, onClose }: Props) {
  const [files, setFiles] = useState<string[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [at, setAt] = useState(0);
  const list = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api
      .files(taskKey)
      .then(setFiles)
      .catch(() => {
        setFiles([]);
        setError(t("El daemon no ha dado la lista de archivos. Si es de antes de ⌘P, reinícialo."));
      });
  }, [taskKey]);

  const matches = useMemo(() => {
    if (!files) return [];
    if (!query.trim()) return files.slice(0, SHOWN);
    return files
      .map((f) => [f, score(query, f)] as const)
      .filter((x): x is readonly [string, number] => x[1] !== null)
      .sort((a, b) => b[1] - a[1])
      .slice(0, SHOWN)
      .map(([f]) => f);
  }, [files, query]);

  useEffect(() => setAt(0), [query]);
  useEffect(() => {
    list.current?.children[at]?.scrollIntoView({ block: "nearest" });
  }, [at]);

  const choose = (path: string | undefined) => {
    if (!path) return;
    onOpen(path);
    onClose();
  };

  return (
    <Dialog
      title={t("Abrir archivo")}
      onClose={onClose}
      size={{ w: 560, h: 440 }}
      footer={
        <div className="qo-hints">
          <span><kbd>↑↓</kbd>{t("moverse")}</span>
          <span><kbd>↵</kbd>{t("abrir")}</span>
          <span><kbd>Esc</kbd>{t("cerrar")}</span>
        </div>
      }
    >
      <input
        className="input qo-input"
        autoFocus
        value={query}
        placeholder={
          files ? tn(files.length, "Buscar {n} archivo…", "Buscar {n} archivos…") : t("Cargando archivos…")
        }
        aria-label={t("Nombre del archivo")}
        onChange={(e) => setQuery(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "ArrowDown" || (e.ctrlKey && e.key === "n")) {
            e.preventDefault();
            setAt((i) => Math.min(i + 1, matches.length - 1));
          } else if (e.key === "ArrowUp" || (e.ctrlKey && e.key === "p")) {
            e.preventDefault();
            setAt((i) => Math.max(i - 1, 0));
          } else if (e.key === "Enter") {
            e.preventDefault();
            choose(matches[at]);
          }
        }}
      />
      <div className="qo-list" ref={list} role="listbox">
        {matches.map((path, i) => {
          const slash = path.lastIndexOf("/");
          const on = matchedAt(query, path);
          return (
            <button
              key={path}
              type="button"
              role="option"
              aria-selected={i === at}
              className={`qo-row ${i === at ? "on" : ""}`}
              onMouseMove={() => setAt(i)}
              onClick={() => choose(path)}
            >
              <span><Marked text={path.slice(slash + 1)} from={slash + 1} on={on} /></span>
              {slash > 0 && <span className="qo-dir"><Marked text={path.slice(0, slash)} from={0} on={on} /></span>}
            </button>
          );
        })}
        {error && <div className="qo-error">{error}</div>}
        {files && !error && matches.length === 0 && <div className="qo-empty">{t("Ningún archivo coincide.")}</div>}
      </div>
    </Dialog>
  );
}
