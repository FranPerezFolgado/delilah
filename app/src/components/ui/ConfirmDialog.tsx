import { t } from "../../i18n";
import { Dialog } from "./Dialog";

export interface ConfirmDialogProps {
  title: string;
  body: string;
  /** Label of the action button. */
  action: string;
  danger?: boolean;
  onConfirm: () => void;
  onClose: () => void;
  /** Why the action is unavailable; shown as a note and disables the action. */
  blocked?: string;
}

/** Footer is always Cancelar · Acción. */
export function ConfirmDialog({ title, body, action, danger, onConfirm, onClose, blocked }: ConfirmDialogProps) {
  return (
    <Dialog
      title={title}
      onClose={onClose}
      footer={
        <>
          <button type="button" className="btn" onClick={onClose}>{t("Cancelar")}</button>
          <button
            type="button"
            className={danger ? "btn btn-danger" : "btn btn-p"}
            disabled={!!blocked}
            onClick={onConfirm}
          >
            {action}
          </button>
        </>
      }
    >
      <p>{body}</p>
      {blocked && <p className="dialog-note">{blocked}</p>}
    </Dialog>
  );
}
