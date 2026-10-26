// Relative times for lists and headers: «hace un momento», «hace 4 min», «ayer», dates after a week.
import { locale, t } from "./i18n";

const MIN = 60_000;
const H = 60 * MIN;

const startOfDay = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();

/** `when` (a Date or an ISO string) as a short phrase relative to `now`; the future counts as now. */
export function relativeTime(when: Date | string, now: Date | number = Date.now()): string {
  const then = typeof when === "string" ? new Date(when) : when;
  const nowDate = new Date(now);
  const diff = Math.max(0, nowDate.getTime() - then.getTime());
  if (diff < MIN) return t("hace un momento");
  const short = new Intl.RelativeTimeFormat(locale(), { style: "short", numeric: "always" });
  if (diff < H) return short.format(-Math.floor(diff / MIN), "minute");
  const days = Math.round((startOfDay(nowDate) - startOfDay(then)) / 86_400_000);
  if (days < 1) return short.format(-Math.floor(diff / H), "hour");
  if (days === 1) return new Intl.RelativeTimeFormat(locale(), { numeric: "auto" }).format(-1, "day");
  if (days < 7) return new Intl.RelativeTimeFormat(locale(), { style: "long", numeric: "always" }).format(-days, "day");
  return then.toLocaleDateString(locale(), { day: "numeric", month: "short" });
}
