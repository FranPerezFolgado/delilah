import { useEffect, useState } from "react";
import { t } from "../i18n";
import { connect } from "../live";
import { requestOpenTask } from "../platform";
import { Button } from "../components/ui";
import { Building } from "./Building";
import { OfficeTab } from "./OfficeTab";

/** The playground in a window of its own: the building, and a floor in place when clicked.
 *  A lead's click brings the main window to the front on that task's terminal. It opens its
 *  own connection to the daemon, the only one: the office data layer shares it and the store
 *  keeps the floors, so drilling down and back is instant. */
export function PlaygroundWindow() {
  const [selected, setSelected] = useState<string | null>(null);
  const [online, setOnline] = useState(true);
  useEffect(() => connect(setOnline), []);
  return (
    <div className="window">
      {selected === null ? (
        <Building onOpenFloor={setSelected} online={online} />
      ) : (
        <>
          <div className="view-head" data-tauri-drag-region>
            <Button variant="secondary" size={28} icon="back" onClick={() => setSelected(null)}>
              {t("Volver al edificio")}
            </Button>
          </div>
          <div className="view-body">
            <OfficeTab owner={selected} online={online} onOpenSession={(session) => void requestOpenTask({ task: selected, session })} />
          </div>
        </>
      )}
    </div>
  );
}
