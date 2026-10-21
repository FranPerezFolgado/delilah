import { useEffect, useMemo, useState } from "react";
import { api, type Project, type Session, type SpecSummary, type Task } from "../api";
import { connect, on } from "../live";
import { groupByProject, type ProjectArea } from "./layout";

/** The playground's data: live sessions with their tasks and projects, fresh from the
 *  change feed. `online` given (the main window, which already holds the feed) is used
 *  as is; without it, this opens its own connection (connect() is not idempotent). */
export function usePlayground(online?: boolean): { areas: ProjectArea[]; online: boolean } {
  const [sessions, setSessions] = useState<Session[]>([]);
  const [tasks, setTasks] = useState<Task[]>([]);
  const [projects, setProjects] = useState<Project[]>([]);
  const [specs, setSpecs] = useState<Record<string, SpecSummary>>({});
  const [own, setOwn] = useState(true);

  useEffect(() => {
    const fetchers = {
      sessions: () => api.sessions().then(setSessions),
      tasks: () => api.tasks().then(setTasks),
      projects: () => api.projects().then(setProjects),
      // An older daemon has no specs: phases fall back to the start.
      specs: () => api.specs().catch(() => ({})).then(setSpecs),
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

  const areas = useMemo(() => groupByProject(sessions, tasks, projects, specs), [sessions, tasks, projects, specs]);
  return { areas, online: online ?? own };
}
