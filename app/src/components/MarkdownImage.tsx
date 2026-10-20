import { useEffect, useState } from "react";
import { t } from "../i18n";
import { safeUrl } from "../markdown";

interface Props {
  src: string;
  alt?: string;
  title?: string;
  /** The workspace file a relative `src` points at, when the document has a workspace. */
  local: string | null;
  /** The document's version: a new one loads a local image again (it may have changed). */
  version?: string;
  load: (path: string) => Promise<string>;
  /** A remote image waits for a click before loading. */
  askFirst: boolean;
}

/** An image in a Markdown document. `https:` ones load directly; workspace ones through
 *  `load` (the CSP allows nothing else); anything else, or one that fails, shows its
 *  alt text and target instead. */
export function MarkdownImage({ src, alt, title, local, version, load, askFirst }: Props) {
  const remote = safeUrl(src) === "external" && /^\s*https:/i.test(src);
  const [dataUrl, setDataUrl] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [asked, setAsked] = useState(false);

  useEffect(() => {
    setDataUrl(null);
    setError(null);
    if (!local) return;
    let cancelled = false;
    load(local)
      .then((url) => !cancelled && setDataUrl(url))
      .catch((e) => !cancelled && setError((e as Error).message));
    return () => {
      cancelled = true;
    };
    // `load` is a new function each render; the image only depends on what it points at.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [local, version]);

  if (error !== null || (!remote && !local)) {
    return (
      <span className="md-img-alt" title={error ?? undefined}>
        {alt || src}
        {alt && src && <span className="mono">{src}</span>}
      </span>
    );
  }
  if (remote && askFirst && !asked) {
    return (
      <button className="btn btn-ghost md-img-ask" onClick={() => setAsked(true)} title={src}>
        {t("Cargar imagen: {name}", { name: alt || src })}
      </button>
    );
  }
  const shown = remote ? src : dataUrl;
  if (!shown) return null;
  return (
    <img
      src={shown}
      alt={alt}
      title={title}
      loading="lazy"
      referrerPolicy="no-referrer"
      onError={() => setError(t("No se pudo cargar la imagen"))}
    />
  );
}
