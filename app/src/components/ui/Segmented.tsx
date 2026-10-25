import { useRef, type KeyboardEvent } from "react";
import { t } from "../../i18n";
import "./Segmented.css";

export interface SegmentedOption {
  value: string;
  label: string;
  /** Small chip next to the label, e.g. "beta". */
  tag?: string;
  disabled?: boolean;
  title?: string;
}

export interface SegmentedProps {
  /** "radio": picks a value (radiogroup). "pressed": mode switch (buttons with aria-pressed). */
  mode: "radio" | "pressed";
  label: string;
  options: SegmentedOption[];
  value: string;
  onChange: (value: string) => void;
}

export function Segmented({ mode, label, options, value, onChange }: SegmentedProps) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);
  const radio = mode === "radio";

  const onKey = (e: KeyboardEvent, i: number) => {
    if (!radio) return;
    const step = e.key === "ArrowRight" || e.key === "ArrowDown" ? 1 : e.key === "ArrowLeft" || e.key === "ArrowUp" ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    for (let k = 1; k <= options.length; k++) {
      const j = (i + step * k + options.length * k) % options.length;
      if (!options[j].disabled) {
        onChange(options[j].value);
        refs.current[j]?.focus();
        return;
      }
    }
  };

  const selected = options.findIndex((o) => o.value === value);
  return (
    <div className="seg" role={radio ? "radiogroup" : "group"} aria-label={label}>
      {options.map((o, i) => {
        const on = o.value === value;
        const tabbable = on || (selected < 0 && i === options.findIndex((x) => !x.disabled));
        return (
          <button
            key={o.value}
            ref={(el) => { refs.current[i] = el; }}
            type="button"
            role={radio ? "radio" : undefined}
            aria-checked={radio ? on : undefined}
            aria-pressed={radio ? undefined : on}
            tabIndex={radio && !tabbable ? -1 : undefined}
            disabled={o.disabled}
            title={o.title}
            onClick={() => onChange(o.value)}
            onKeyDown={(e) => onKey(e, i)}
          >
            {o.label}
            {o.tag && <span className="tag">{t(o.tag)}</span>}
          </button>
        );
      })}
    </div>
  );
}
