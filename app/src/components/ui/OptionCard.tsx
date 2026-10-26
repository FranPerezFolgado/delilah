import type { ButtonHTMLAttributes } from "react";
import "./OptionCard.css";

export interface OptionCardProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "title"> {
  checked: boolean;
  title: string;
  description?: string;
  /** Group name; the cards live inside a `role="radiogroup"` supplied by the caller. */
  name: string;
}

export function OptionCard({ checked, title, description, name, className, ...rest }: OptionCardProps) {
  return (
    <button type="button" role="radio" aria-checked={checked} data-name={name} className={`opt ${className ?? ""}`.trim()} {...rest}>
      <strong>{title}</strong>
      {description && <span className="muted small">{description}</span>}
    </button>
  );
}
