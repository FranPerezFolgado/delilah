import "./Toggle.css";

export interface ToggleProps {
  label: string;
  pressed: boolean;
  onChange: (pressed: boolean) => void;
  disabled?: boolean;
  /** Reason when disabled, or a hint. */
  title?: string;
  /** Narrow layouts: only the switch, the label stays as the accessible name. */
  iconOnly?: boolean;
}

export function Toggle({ label, pressed, onChange, disabled, title, iconOnly }: ToggleProps) {
  return (
    <button
      type="button"
      className="toggle"
      aria-pressed={pressed}
      aria-label={iconOnly ? label : undefined}
      disabled={disabled}
      title={title ?? (iconOnly ? label : undefined)}
      onClick={() => onChange(!pressed)}
    >
      <span className="sw" aria-hidden="true" />
      {!iconOnly && <span>{label}</span>}
    </button>
  );
}
