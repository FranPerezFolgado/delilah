import type { ButtonHTMLAttributes } from "react";
import { Icon, type IconName } from "./Icon";
import "./Button.css";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
const VARIANT: Record<ButtonVariant, string> = { primary: "b-p", secondary: "b-s", ghost: "b-g", danger: "b-d" };

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant: ButtonVariant;
  /** Height in px: 32 (default) or 28 (`.sm`, font 12). */
  size?: 32 | 28;
  icon?: IconName;
}

/** A disabled button should carry a `title` with the reason. */
export function Button({ variant, size = 32, icon, className, children, type = "button", ...rest }: ButtonProps) {
  const cls = ["b", VARIANT[variant], size === 28 ? "sm" : "", className ?? ""].filter(Boolean).join(" ");
  return (
    <button type={type} className={cls} {...rest}>
      {icon && <Icon name={icon} />}
      {children}
    </button>
  );
}

export interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "aria-label" | "children"> {
  label: string;
  icon: IconName;
  /** When given, the button is a mode switch and gets `aria-pressed`. */
  pressed?: boolean;
}

export function IconButton({ label, icon, pressed, className, type = "button", ...rest }: IconButtonProps) {
  return (
    <button type={type} className={`ib ${className ?? ""}`.trim()} aria-label={label} aria-pressed={pressed} {...rest}>
      <Icon name={icon} />
    </button>
  );
}
