import { useEffect, useMemo, useRef, useState } from "react";
import { api, type Activity, type Project, type Session, type SpecSummary, type Task } from "../api";
import { connect, on } from "../live";
import { groupByProject, slotMaps, type ProjectArea } from "./layout";

/** The playground's data: live sessions with their tasks and projects, fresh from the
 *  change feed. `online` given (the main window, which already holds the feed) is used
 *  as is; without it, this opens its own connection (connect() is not idempotent). */
export function usePlayground(online?: boolean): { areas: ProjectArea[]; online: boolean } {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [specs, setSpecs] = useState<Record<string, SpecSummary>>({});
  const [activity, setActivity] = useState<Record<string, Activity>>({});
  const [own, setOwn] = useState(true);

  useEffect(() => {
    const fetchers = {
      sessions: () => api.sessions().then(setSessions),
      tasks: () => api.tasks().then(setTasks),
      projects: () => api.projects().then(setProjects),
      // An older daemon has no specs: phases fall back to the start.
      specs: () => api.specs().catch(() => ({})).then(setSpecs),
      // An older daemon has no activity: every session reads as idle.
      activity: () => api.activity().catch(() => ({ sessions: {} })).then((a) => setActivity(a.sessions)),
    };
    const topics = Object.keys(fetchers) as (keyof typeof fetchers)[];
    topics.forEach((t) => fetchers[t]().catch(() => {}));
    const offs = topics.map((t) => on(t, () => fetchers[t]().catch(() => {})));
    return () => offs.forEach((off) => off());
  }, []);

  const external = online !== undefined;
  useEffect(() => {
    if (external) return;
    return connect(setOwn);
  }, [external]);

  // Slots live here, not in the view, so they survive it unmounting.
  const slots = useRef<Map<string, Map<string, number>>>(new Map());
  // Mutating the ref here is safe: regrouping the same inputs gives the same slots.
  const areas = useMemo(() => {
    const next = groupByProject(sessions, tasks, projects, specs, slots.current);
    slots.current = slotMaps(next);
    return next;
  }, [sessions, tasks, projects, specs]);
  const withActivity = useMemo(
    () => areas.map((a) => ({ ...a, desks: a.desks.map((d) => ({ ...d, activity: activity[d.session.id] })) })),
    [areas, activity],
  );
  return { areas: withActivity, online: online ?? own };
}
