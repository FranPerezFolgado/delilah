import { beforeEach, describe, expect, it } from "vitest";
import { setLanguage } from "./i18n";
import { isPhased, workflowPresets } from "./workflowPresets";
import { DEFAULT_PROJECT_SETTINGS } from "./api";

const phased = { ...DEFAULT_PROJECT_SETTINGS, workflow: { kind: "phased" as const, plan: "a", implement: "b", review: "c" } };

describe("workflowPresets", () => {
  beforeEach(() => setLanguage("en"));

  it("has the four presets in order", () => {
    expect(workflowPresets().map((p) => p.id)).toEqual(["builtin", "nothing", "speckit-feature", "speckit-official"]);
  });

  it("builtin and nothing are single", () => {
    const [b, n] = workflowPresets();
    expect(b.workflow).toEqual({ kind: "single", start: { type: "builtin" } });
    expect(n.workflow).toEqual({ kind: "single", start: { type: "nothing" } });
  });

  it("the /feature preset matches the daemon in both languages", () => {
    setLanguage("en");
    const en = workflowPresets()[2].workflow;
    expect(en).toEqual({
      kind: "phased",
      plan: "/feature {task}",
      implement: "/orchestrate {spec_dir}",
      review:
        "/review the branch of this task. Then apply the confirmed findings, update the docs they made stale and commit. Do not push or open a PR.",
    });
    setLanguage("es");
    const es = workflowPresets()[2].workflow;
    expect(es.kind === "phased" && es.review).toBe(
      "/review de la rama de esta tarea. Después aplica los hallazgos confirmados, actualiza la documentación que hayan dejado obsoleta y haz commit. No hagas push ni abras una PR.",
    );
  });

  it("the official preset uses Spec Kit's commands", () => {
    const w = workflowPresets()[3].workflow;
    expect(w.kind === "phased" && [w.plan, w.implement]).toEqual(["/speckit-specify {title}", "/speckit-implement"]);
  });
});

describe("isPhased", () => {
  it("is true for project or legacy feature with a phased workflow", () => {
    expect(isPhased("project", phased)).toBe(true);
    expect(isPhased("feature", phased)).toBe(true);
  });
  it("is false for prompt/none tasks", () => {
    expect(isPhased("prompt", phased)).toBe(false);
    expect(isPhased("none", phased)).toBe(false);
  });
  it("is false without a project workflow or with a single one", () => {
    expect(isPhased("project", DEFAULT_PROJECT_SETTINGS)).toBe(false);
    expect(isPhased("project", { ...DEFAULT_PROJECT_SETTINGS, workflow: null })).toBe(false);
    expect(isPhased("project", { workflow: { kind: "single", start: { type: "nothing" } } })).toBe(false);
    expect(isPhased("project", undefined)).toBe(false);
  });
});
