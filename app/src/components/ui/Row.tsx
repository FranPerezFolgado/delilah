import type { ButtonHTMLAttributes, ReactNode } from "react";
import "./Row.css";

type Native = Omit<ButtonHTMLAttributes<HTMLElement>, "title"> & { href?: string };

export interface RowProps extends Native {
  as?: "button" | "a";
  current?: boolean;
  selected?: boolean;
  leading?: ReactNode;
  trailing?: ReactNode;
  /** Main text. */
  title?: ReactNode;
  /** Second line; makes the row the 52 px two-line variant. */
  sub?: ReactNode;
}

/** The one list row (32 px; 52 px with `sub`). */
export function Row({ as = "button", current, selected, leading, trailing, title, sub, children, className, ...rest }: RowProps) {
  const cls = ["row", sub != null ? "row-2" : "", className ?? ""].filter(Boolean).join(" ");
  const body = (
    <>
      {leading != null && <span className="row-lead">{leading}</span>}
      <span className="row-main">
        <span className="row-title">{title ?? children}</span>
        {sub != null && <span className="row-sub">{sub}</span>}
      </span>
      {trailing != null && <span className="row-trail">{trailing}</span>}
    </>
  );
  const aria = {
    "aria-current": current ? ("true" as const) : undefined,
    "aria-selected": selected ? ("true" as const) : undefined,
  };
  if (as === "a") {
    return (
      <a className={cls} {...aria} {...(rest as object)}>
        {body}
      </a>
    );
  }
  return (
    <button type="button" className={cls} {...aria} {...rest}>
      {body}
    </button>
  );
}
