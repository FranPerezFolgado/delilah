import { useEffect, useState, type KeyboardEvent } from "react";
import { t } from "../i18n";
import { Icon, type IconName } from "../components/ui/Icon";
import { useFloor } from "./useOffice";
import { Floor } from "./Floor";
import { FlowView } from "./FlowView";
import "../components/ui/Segmented.css";
import "./OfficeTab.css";

export interface OfficeTabProps {
  /** A task key or `@project`. */
  owner: string;
  online?: boolean;
  onOpenSession(sessionId: string): void;
}

type View = "office" | "flow";
const KEY = "delilah.office.view";
const EVENT = "delilah:office-view";

const read = (): View => {
  try {
    return localStorage.getItem(KEY) === "flow" ? "flow" : "office";
  } catch {
    return "office";
  }
};

/** The chosen view, shared by every OfficeTab and kept in localStorage (default Oficina). */
function useOfficeView(): [View, (v: View) => void] {
  const [view, setView] = useState<View>(read);
  useEffect(() => {
    const on = (e: Event) => setView((e as CustomEvent<View>).detail);
    window.addEventListener(EVENT, on);
    return () => window.removeEventListener(EVENT, on);
  }, []);
  const choose = (v: View) => {
    try {
      localStorage.setItem(KEY, v);
    } catch { /* kept for this session only */ }
    setView(v);
    window.dispatchEvent(new CustomEvent(EVENT, { detail: v }));
  };
  return [view, choose];
}

function ViewToggle({ view, onChange }: { view: View; onChange(v: View): void }) {
  const tabs: { value: View; label: string; icon: IconName }[] = [
    { value: "office", label: t("Oficina"), icon: "office" },
    { value: "flow", label: t("Flujo"), icon: "flow" },
  ];
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      e.preventDefault();
      onChange(view === "office" ? "flow" : "office");
    }
  };
  return (
    <div className="seg office-seg" role="tablist" aria-label={t("Vista")} onKeyDown={onKey}>
      {tabs.map((o) => (
        <button
          key={o.value}
          type="button"
          role="tab"
          aria-selected={o.value === view}
          tabIndex={o.value === view ? 0 : -1}
          onClick={() => onChange(o.value)}
        >
          <Icon name={o.icon} />
          {o.label}
        </button>
      ))}
    </div>
  );
}

/** The Oficina/Flujo tab of a task or project screen: the owner's floor or its flow graph. */
export function OfficeTab({ owner, online, onOpenSession }: OfficeTabProps) {
  const { floor, online: on } = useFloor(owner, online);
  const [view, setView] = useOfficeView();
  const toggle = <ViewToggle view={view} onChange={setView} />;
  return (
    <div className="office-tab">
      {view === "flow" ? (
        <FlowView owner={owner} online={on} headerExtra={toggle} />
      ) : (
        <Floor floor={floor} online={on} onOpenSession={onOpenSession} headerExtra={toggle} />
      )}
    </div>
  );
}
