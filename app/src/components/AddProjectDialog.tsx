import { useState } from "react";
import { api, type FolderInspection, type ProjectKind } from "../api";
import { isDesktop, pickFolder } from "../platform";
import { t } from "../i18n";
import { Button, Dialog, OptionCard } from "./ui";

interface Props {
  onClose: () => void;
  onAdded: (name: string) => void;
}

/** Adds a project: the folder, an optional name, and whether it is one repo or a parent of several. */
export function AddProjectDialog({ onClose, onAdded }: Props) {
  const [path, setPath] = useState("");
  const [kind, setKind] = useState<ProjectKind>("repo");
  const [name, setName] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [found, setFound] = useState<FolderInspection | null>(null);

  // Fill the rest of the form from what the folder is.
  const inspect = async (folder: string) => {
    setFound(null);
    if (!folder.trim()) return;
    setPath(folder.trim());
    try {
      const info = await api.inspectFolder(folder.trim());
      setFound(info);
      setPath(info.path);
      setKind(info.kind);
      setName((n) => n || info.name);
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const choose = async () => {
    const folder = await pickFolder(t("Elige la carpeta del repositorio"));
    if (folder) {
      setName("");
      await inspect(folder);
    }
  };

  const add = async (e: React.FormEvent) => {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const p = await api.addProject({ path: path.trim(), kind, name: name.trim() || undefined });
      onAdded(p?.name ?? name.trim());
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setBusy(false);
    }
  };

  const shown = error && (error.includes("not a git repo") ? t("Esa carpeta no es un repositorio git") : error);

  return (
    <Dialog
      title={t("Añadir proyecto")}
      onClose={onClose}
      footer={
        <>
          <Button variant="secondary" onClick={onClose}>{t("Cancelar")}</Button>
          <Button variant="primary" type="submit" form="add-project-form" disabled={busy || !path.trim()}>{t("Añadir")}</Button>
        </>
      }
    >
      <form id="add-project-form" className="apd" onSubmit={add}>
        <label className="field">
          <span>{t("Carpeta")}</span>
          <span className="apd-folder">
            <input
              className="input mono apd-path"
              placeholder={t("/Users/tu/dev/projects/mi-proyecto")}
              value={path}
              onChange={(e) => setPath(e.target.value)}
              onBlur={(e) => inspect(e.target.value)}
              required
            />
            {isDesktop && <Button variant="secondary" onClick={choose}>{t("Elegir carpeta…")}</Button>}
          </span>
          {found && (
            <span className="muted small">
              {found.submodules > 0
                ? t("Repo con {n} submódulos: se añadirá como multi-repo.", { n: found.submodules })
                : t("Repo sin submódulos: se añadirá como repo.")}
            </span>
          )}
        </label>
        <label className="field">
          <span>{t("Nombre")}</span>
          <input className="input" placeholder={t("Por defecto, el de la carpeta")} value={name} onChange={(e) => setName(e.target.value)} />
        </label>
        <div className="field">
          <span id="apd-kind">{t("Qué es")}</span>
          <div className="apd-kinds" role="radiogroup" aria-labelledby="apd-kind">
            <OptionCard name="kind" checked={kind === "repo"} title="Repo" description={t("Un repositorio git")} onClick={() => setKind("repo")} />
            <OptionCard name="kind" checked={kind === "parent"} title="Multi-repo" description={t("Un repo con submódulos")} onClick={() => setKind("parent")} />
          </div>
        </div>
        {shown && <p className="apd-error selectable" role="alert">{shown}</p>}
      </form>
    </Dialog>
  );
}
