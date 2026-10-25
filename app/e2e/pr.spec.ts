import { mkdirSync, renameSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, test } from "./fixtures";

// The UI follows the browser language and these specs read the Spanish texts.
test.use({ locale: "es-ES" });

// The fake `gh` JSON the daemon parses, mirroring crates/delilah-e2e/src/fixtures.rs (FakeGh).
const REPO_URL = "https://github.com/o/r";

function put(dir: string, name: string, value: unknown) {
  const tmp = path.join(dir, `.${name}.tmp`);
  writeFileSync(tmp, JSON.stringify(value, null, 2));
  renameSync(tmp, path.join(dir, name));
}

function openFailingPr(dir: string, n: number, branch: string, title: string) {
  mkdirSync(dir, { recursive: true });
  const url = `${REPO_URL}/pull/${n}`;
  put(dir, "repo.json", { nameWithOwner: "o/r", url: REPO_URL, defaultBranchRef: { name: "main" } });
  put(dir, "user.json", { login: "e2e-user" });
  put(dir, "pr-list.json", [
    {
      number: n, url, state: "OPEN", updatedAt: "2026-10-20T10:00:00Z", headRefName: branch,
      baseRefName: "main", title, isDraft: false,
    },
  ]);
  put(dir, `pr-${n}.json`, {
    data: {
      repository: {
        pullRequest: {
          state: "OPEN", isDraft: false, merged: false, headRefOid: `sha-${n}-1`, baseRefName: "main",
          title, url, reviewDecision: "CHANGES_REQUESTED",
          commits: {
            nodes: [
              {
                commit: {
                  statusCheckRollup: {
                    contexts: {
                      nodes: [
                        { __typename: "CheckRun", name: "build", status: "COMPLETED", conclusion: "FAILURE", detailsUrl: "https://ci.invalid/1" },
                      ],
                    },
                  },
                },
              },
            ],
          },
          reviews: {
            nodes: [
              { databaseId: 100, state: "CHANGES_REQUESTED", author: { login: "reviewer" }, body: "Please fix the build", submittedAt: "2026-10-20T10:00:00Z" },
            ],
          },
          comments: {
            nodes: [
              { databaseId: 200, author: { login: "reviewer" }, body: "Looks odd here", createdAt: "2026-10-20T10:30:00Z" },
            ],
          },
          reviewThreads: { nodes: [] },
        },
      },
    },
  });
}

test("the PR tab shows the PR state, the failing check and the review comment", async ({ page, env, api, project }) => {
  const p = await project("repo");
  const title = `PR flow ${Date.now()}`;
  // A free task starts without a branch unless given one (decision 65).
  const branch = `feature/pr-flow-${Date.now()}`;
  await api.post<{ key: string }>("/api/v1/runs", {
    key: "", title, project: p.name, submodules: [], free: true, branch,
  });
  openFailingPr(env.gh_dir, 7, branch, `PR for ${title}`);

  await page.goto("/");
  await page.getByRole("button", { name: /^Tareas/ }).click();
  await page.locator(".board .kcard", { hasText: title }).click();
  await page.getByRole("tab", { name: "PR", exact: true }).click();

  const card = page.locator(".pr-view .pr-card");
  await expect(card).toBeVisible({ timeout: 30_000 });
  await expect(card.locator(".pr-chip")).toHaveText("Abierta");
  await expect(card.locator(".pr-meta")).toContainText("Cambios pedidos");
  await expect(card.locator(".pr-meta")).toContainText("1 comentarios");
  await expect(card.locator(".pr-meta")).toContainText("CI fallido");
  const check = card.locator(".pr-checks li", { hasText: "build" });
  await expect(check.locator(".pr-ci-failure")).toBeVisible();
  await expect(check).toContainText("failure");
});
