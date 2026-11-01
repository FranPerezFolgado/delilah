# Dependabot: assessment and proposal (FR-016)

**Status: proposal for the owner. `.github/dependabot.yml` is not changed by this feature unless the owner approves (FR-017).**

## What it has produced (as of 2026-11-01)

| PR | State | Group | Failing checks |
|---|---|---|---|
| #19 | Closed | actions, 4 updates | 0 |
| #20 | Open | tauri, 3 updates | 0 |
| #21 | Open | tauri, 3 updates | 1 |
| #22 | Open | all, 9 updates | 6 |
| #23 | Open | all, 5 updates | 0 |
| #25 | Open | actions | 0 |

Six PRs, all opened on 2026-10-26: 5 open, 1 closed, 0 merged. That is a single run: with a weekly schedule, the next one is not due until about 2026-11-02.

## Current configuration

Weekly on four ecosystems (cargo at `/`, cargo at `/app/src-tauri`, npm at `/app`, GitHub Actions), up to 5 PRs each. Updates are already grouped, with a separate `tauri` group.

## Assessment

Probably more noise than value at this pace, but the evidence is thin. One run produced six PRs, none has been merged in the 6 days since, and two have failing checks. The argument rests mostly on the setup: four weekly schedules are a lot for a one-person project to review.

## Recommendation

**Go monthly, keep the groups, and limit each ecosystem to 2 PRs.** This keeps a regular prompt to update without a weekly queue.

- **Trade-off**: non-security updates arrive up to a month late, and monthly batches are larger, so a failing group takes longer to untangle.
- **Separate point to check**: `GET /repos/FranPerezFolgado/delilah/vulnerability-alerts` returned 404, which normally means vulnerability alerts are off. If so, a monthly schedule would be the only signal of a vulnerable dependency, so turning alerts on is worth doing alongside this. It's a repository setting, not part of this feature.
- **Alternatives**: keep weekly (the current noise); remove dependabot (no prompt at all for a project that ships a public installer).
- **The 5 open PRs**: the owner decides whether to merge or close them; this feature leaves them alone.

## If approved

For each of the four entries in `.github/dependabot.yml`, set `interval: "monthly"` and `open-pull-requests-limit: 2`. No other change.
