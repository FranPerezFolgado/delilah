import { relativeTime } from "../time";
import { t } from "../i18n";
import { Button } from "./ui";
import "./DisconnectedBanner.css";

export interface DisconnectedBannerProps {
  /** Epoch ms of the last successful request, or null if none yet. */
  lastOk: number | null;
  onRetry: () => void;
}

/** Shown over the main area while delilahd is unreachable; the shell greys the content and disables actions. */
export function DisconnectedBanner({ lastOk, onRetry }: DisconnectedBannerProps) {
  return (
    <div className="dcb" role="alert">
      <span className="dot dcb-dot" />
      <div className="dcb-text">
        <strong>{t("Sin conexión con delilahd · reintentando…")}</strong>
        <span>
          {t("Para iniciarlo, abre la app de Delilah o ejecuta")} <code>delilahd</code>.
          {lastOk != null && <> {t("Datos de {when}", { when: relativeTime(new Date(lastOk)) })}</>}
        </span>
      </div>
      <Button variant="primary" size={28} onClick={onRetry}>{t("Reintentar ahora")}</Button>
    </div>
  );
}
