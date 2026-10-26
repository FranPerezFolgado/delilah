import { useFloor } from "./useOffice";
import { Floor } from "./Floor";
import "./OfficeTab.css";

export interface OfficeTabProps {
  /** A task key or `@project`. */
  owner: string;
  online?: boolean;
  onOpenSession(sessionId: string): void;
}

/** The Oficina tab of a task or project screen: the owner's floor. */
export function OfficeTab({ owner, online, onOpenSession }: OfficeTabProps) {
  const { floor, online: on } = useFloor(owner, online);
  return (
    <div className="office-tab">
      <Floor floor={floor} online={on} onOpenSession={onOpenSession} />
    </div>
  );
}
