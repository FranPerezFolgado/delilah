import { t } from "../i18n";
import { openPlaygroundWindow } from "../platform";
import { Button } from "../components/ui";
import { Building } from "./Building";

interface Props {
  /** A floor was clicked: its owner (a task key or `@project`). */
  onOpenFloor: (owner: string) => void;
  /** The app's connection state, when it already holds the change feed. */
  online?: boolean;
  /** Already in its own window: no "open in window" button. */
  standalone?: boolean;
}

/** The playground view of the main window: the building, plus the button for its own window. */
export function PlaygroundView({ onOpenFloor, online, standalone }: Props) {
  return (
    <Building
      onOpenFloor={onOpenFloor}
      online={online}
      headerExtra={
        !standalone && (
          <Button variant="secondary" size={28} icon="open" onClick={() => void openPlaygroundWindow()}>
            {t("Abrir en ventana")}
          </Button>
        )
      }
    />
  );
}
