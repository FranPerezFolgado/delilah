import { requestOpenTask } from "../platform";
import { PlaygroundView } from "./PlaygroundView";

/** The playground in a window of its own: the office, nothing else. A click brings the
 *  main window to the front on that task. It opens its own connection to the daemon. */
export function PlaygroundWindow() {
  return (
    <div className="window">
      <PlaygroundView standalone onOpen={(task, session) => void requestOpenTask({ task, session })} />
    </div>
  );
}
