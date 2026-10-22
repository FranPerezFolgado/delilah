//! Autopilot: the chain of a task (plan, implement, review, PR) runs by itself. It
//! rides on saving mode's handoffs (`handoff.rs`): this module only changes who
//! confirms a handoff (`shape`), what each session is told (`step_note`), and what
//! happens after the review (`finish`: read the outcome file, open the draft PRs).
//! The state lives in `tasks.autopilot`, written before each side effect, so a
//! restart repeats nothing (`recover`). See `specs/005-autopilot`.

use std::path::{Path, PathBuf};

use chrono::Utc;
use delilah_core::{Autopilot, AutopilotStage, AutopilotState, Event, Step, Task, TaskKey};

use crate::api::ApiError;
use crate::events::Topic;
use crate::handoff::Boundary;
use crate::lang::Lang;
use crate::pr_types::PrState;
use crate::preview::PreviewState;
use crate::state::AppState;

/// Whether the task is on autopilot and not paused or done.
pub(crate) fn is_running(task: &Task) -> bool {
    matches!(&task.autopilot, Some(a) if a.state == AutopilotState::Running)
}

/// What a boundary becomes on autopilot: the offer of the implementation is taken on
/// the user's behalf. Paused, done or off, the boundary is left as it is.
pub(crate) fn shape(task: &Task, found: Option<Boundary>) -> Option<Boundary> {
    match found {
        Some(Boundary::Offer(to)) if is_running(task) => Some(Boundary::Start(to)),
        other => other,
    }
}

/// Where the review session `session` writes its outcome.
pub(crate) fn outcome_path(app: &AppState, key: &TaskKey, session: &str) -> PathBuf {
    app.config
        .task_dir(key.as_str())
        .join("autopilot")
        .join(format!("review-{session}.json"))
}

/// What a step's session is told on top of its prompt when the task is on autopilot.
/// `outcome` is where the review writes its outcome. The plan step has its own note
/// (`handoff::autopilot_plan_note`).
pub(crate) fn step_note(lang: Lang, step: Step, outcome: &Path) -> Option<String> {
    let path = outcome.display();
    match step {
        Step::Plan => None,
        Step::Implement => Some(
            lang.pick(
                "Autopilot: nadie va a responder ni aprobar nada. Implementa todas las tareas sin parar entre fases ni entre paquetes de trabajo y sin pedir confirmación; si una pregunta tiene una opción recomendada, elige esa. Para solo cuando todas las tareas estén marcadas.",
                "Autopilot: nobody will answer or approve anything. Implement all the tasks without stopping between phases or work packages and without asking for confirmation; if a question has a recommended option, pick it. Stop only once every task is checked.",
            )
            .to_string(),
        ),
        Step::Review => Some(match lang {
            Lang::Es => format!(
                "Autopilot: nadie va a responder ni aprobar nada. Ejecuta los checks del proyecto y arregla lo que falle, aplica los hallazgos confirmados, haz commit y escribe el título y la descripción de la PR (por ejemplo con `pr-writer`). Termina siempre con un resultado explícito: escribe en `{path}` un JSON `{{\"outcome\":\"ready\",\"pr_title\":\"...\",\"pr_body\":\"...\"}}` si la rama está lista, o `{{\"outcome\":\"blocked\",\"reason\":\"...\"}}` con el motivo si no puedes dejarla lista. No hagas push ni abras una PR: lo hace Delilah."
            ),
            Lang::En => format!(
                "Autopilot: nobody will answer or approve anything. Run the project's checks and fix what fails, apply the confirmed findings, commit, and write the PR title and description (for example with `pr-writer`). Always end with an explicit outcome: write to `{path}` a JSON `{{\"outcome\":\"ready\",\"pr_title\":\"...\",\"pr_body\":\"...\"}}` if the branch is ready, or `{{\"outcome\":\"blocked\",\"reason\":\"...\"}}` with the reason if you cannot get it ready. Do not push or open a PR: Delilah does it."
            ),
        }),
    }
}

/// What the review reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    Ready {
        pr_title: Option<String>,
        pr_body: Option<String>,
    },
    Blocked {
        reason: String,
    },
}

#[derive(serde::Deserialize)]
struct OutcomeFile {
    outcome: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    pr_title: Option<String>,
    #[serde(default)]
    pr_body: Option<String>,
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.trim().is_empty())
}

/// The outcome in `path`. A missing or invalid file is `Blocked`.
pub(crate) fn read_outcome(lang: Lang, path: &Path) -> Outcome {
    let missing = || Outcome::Blocked {
        reason: lang
            .pick(
                "la revisión no informó de un resultado",
                "the review did not report an outcome",
            )
            .to_string(),
    };
    let Some(file) = std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<OutcomeFile>(&text).ok())
    else {
        return missing();
    };
    match file.outcome.as_str() {
        "ready" => Outcome::Ready {
            pr_title: non_empty(file.pr_title),
            pr_body: non_empty(file.pr_body),
        },
        "blocked" => Outcome::Blocked {
            reason: non_empty(file.reason).unwrap_or_else(|| {
                lang.pick(
                    "la revisión quedó bloqueada",
                    "the review reported it is blocked",
                )
                .to_string()
            }),
        },
        _ => missing(),
    }
}

async fn write_state(app: &AppState, key: &TaskKey, autopilot: &Autopilot) -> bool {
    if let Err(e) = app.store.set_autopilot(key, Some(autopilot)).await {
        tracing::warn!(task = %key, "cannot record the autopilot: {e}");
        return false;
    }
    app.emit(Topic::Tasks);
    true
}

/// The review turn ended: if it is ready, push and open the draft PRs and finish. Only
/// one finish per task runs at a time, and a finished (or paused) autopilot does
/// nothing, so a repeated turn end opens no second PR.
pub(crate) async fn finish(app: &AppState, key: &TaskKey, session: &str) {
    if !app.handoffs.mark_finishing(key) {
        return;
    }
    run_finish(app, key, session).await;
    app.handoffs.unmark_finishing(key);
}

async fn run_finish(app: &AppState, key: &TaskKey, session: &str) {
    let Ok(Some(task)) = app.store.get_task(key).await else {
        return;
    };
    let Some(mut autopilot) = task.autopilot.clone().filter(|_| is_running(&task)) else {
        return;
    };
    let opening = autopilot.stage == Some(AutopilotStage::OpeningPr);
    let reopening = opening || autopilot.stage == Some(AutopilotStage::Preview);
    let (mut title, mut body) = (None, None);
    match read_outcome(app.lang(), &outcome_path(app, key, session)) {
        Outcome::Ready { pr_title, pr_body } => (title, body) = (pr_title, pr_body),
        // The file is only needed for the agent's title and description once the PRs
        // are being opened.
        Outcome::Blocked { .. } if reopening => {}
        Outcome::Blocked { reason } => {
            // TODO(WP-09): pause the autopilot with this reason.
            tracing::warn!(task = %key, "the review is blocked: {reason}");
            return;
        }
    }
    // A preview that failed never stops the PR. A restart at `opening_pr` skips the
    // step, taking the URL from the preview if it is still up.
    if !opening && !preview_step(app, key, &mut autopilot).await {
        return;
    }
    let preview_url = app
        .previews
        .view(key.as_str())
        .filter(|v| v.state == PreviewState::Running)
        .and_then(|v| v.url);
    if !opening {
        autopilot.stage = Some(AutopilotStage::OpeningPr);
        if !write_state(app, key, &autopilot).await {
            return;
        }
    }
    // Links a PR that exists already (a restart in the middle of opening, or one made
    // by hand), so none is opened twice.
    crate::pr_loop::check_task(app, key, true).await;
    if let Err(e) = open_prs(
        app,
        key,
        title.as_deref(),
        body.as_deref(),
        preview_url.as_deref(),
    )
    .await
    {
        // TODO(WP-09): pause the autopilot with this reason.
        tracing::warn!(task = %key, "autopilot cannot open the PR: {}", e.message());
        return;
    }
    let urls = linked_urls(app, key).await;
    if urls.is_empty() {
        // TODO(WP-09): pause: there is nothing to open a PR with.
        tracing::warn!(task = %key, "autopilot found no commits to open a PR with");
        return;
    }
    autopilot.state = AutopilotState::Done;
    autopilot.stage = None;
    autopilot.since = Utc::now();
    if !write_state(app, key, &autopilot).await {
        return;
    }
    if let Err(e) = app
        .transition(key, Event::AutopilotFinished { prs: urls })
        .await
    {
        tracing::warn!(task = %key, "cannot record the autopilot's end: {e}");
    }
}

/// The longest the finish waits for a preview to print its URL: a little more than the
/// preview's own timeout, which fails it, so this is only a safety net.
const PREVIEW_WAIT: std::time::Duration = std::time::Duration::from_secs(11 * 60);

/// Starts the project's preview, if it has a command, and waits until it runs or
/// fails. `false` only if the state cannot be recorded.
async fn preview_step(app: &AppState, key: &TaskKey, autopilot: &mut Autopilot) -> bool {
    let command = match app.store.get_task(key).await {
        Ok(Some(task)) => match &task.project {
            Some(name) => app
                .store
                .project(name)
                .await
                .ok()
                .and_then(|p| p.preview_command),
            None => None,
        },
        _ => None,
    };
    if command.is_none() {
        return true;
    }
    autopilot.stage = Some(AutopilotStage::Preview);
    if !write_state(app, key, autopilot).await {
        return false;
    }
    match crate::preview::start_for(app, key).await {
        Ok(_) => {}
        // Already up (the user started it): use it.
        Err(e) if e.reason.as_deref() == Some("running") => {}
        Err(e) => {
            tracing::warn!(task = %key, "autopilot cannot start the preview: {}", e.message());
            return true;
        }
    }
    let deadline = tokio::time::Instant::now() + PREVIEW_WAIT;
    while tokio::time::Instant::now() < deadline {
        match app.previews.view(key.as_str()) {
            Some(v) if v.state == PreviewState::Starting => {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
            _ => break,
        }
    }
    true
}

fn is_open(row: &crate::store_prs::PrRow) -> bool {
    row.snapshot
        .as_ref()
        .is_none_or(|s| s.state == PrState::Open)
}

/// The URLs of the task's open PRs.
async fn linked_urls(app: &AppState, key: &TaskKey) -> Vec<String> {
    let rows = app.store.prs_of(key).await.unwrap_or_default();
    rows.iter()
        .filter(|r| is_open(r))
        .map(|r| r.url.clone())
        .collect()
}

/// Opens a draft PR in each repo that has commits and no open PR.
async fn open_prs(
    app: &AppState,
    key: &TaskKey,
    title: Option<&str>,
    body: Option<&str>,
    preview_url: Option<&str>,
) -> Result<(), ApiError> {
    let Some((_, ws)) = app.store.workspace(key).await? else {
        return Err(ApiError::conflict(format!("{key} has no workspace")));
    };
    for (repo, _) in ws.repos() {
        let rows = app.store.prs_of(key).await?;
        if rows.iter().any(|r| r.repo == repo && is_open(r)) {
            continue;
        }
        // The proposal says whether the repo has commits to open a PR with, and is
        // what the PR says when the agent wrote no title or description.
        let (proposed_title, proposed_body) =
            match crate::pr_api::proposal_for(app, key, Some(&repo)).await {
                Ok(p) => p,
                Err(e) if matches!(e.reason.as_deref(), Some("no-commits" | "has-pr")) => continue,
                Err(e) => return Err(e),
            };
        let title = title.map_or(proposed_title, str::to_string);
        let mut body = body.map_or(proposed_body, str::to_string);
        if let Some(url) = preview_url {
            body = format!("{}\n\nPreview: {url}\n", body.trim_end());
        }
        crate::pr_api::open_for(app, key, Some(&repo), &title, &body, true).await?;
    }
    Ok(())
}

/// Finishes what a restart left half done: a preview being started or PRs being opened.
pub(crate) async fn recover(app: &AppState, task: &Task) {
    let pending = matches!(
        &task.autopilot,
        Some(a) if a.state == AutopilotState::Running
            && matches!(a.stage, Some(AutopilotStage::OpeningPr | AutopilotStage::Preview))
    );
    if !pending {
        return;
    }
    // A preview still starting is being waited on by a finish that is alive.
    let starting = app
        .previews
        .view(task.key.as_str())
        .is_some_and(|v| v.state == PreviewState::Starting);
    if starting && matches!(&task.autopilot, Some(a) if a.stage == Some(AutopilotStage::Preview)) {
        return;
    }
    if let Some(primary) = crate::handoff::primary_session(app, &task.key).await {
        finish(app, &task.key, &primary.id).await;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::gh::fake::{FakeGh, PR_LIST, PR_URL, REPO_VIEW, bare_snapshot};
    use crate::pr_testkit::{TempDir, git_repo_with_origin, task_with_workspace};
    use crate::store_sessions::{SessionKind, SessionRecord};

    fn running() -> Autopilot {
        Autopilot {
            state: AutopilotState::Running,
            stage: None,
            reason: None,
            since: Utc::now(),
            nudged: None,
        }
    }

    fn task_with(autopilot: Option<AutopilotState>) -> Task {
        let mut t = Task::new(TaskKey::new("PROJ-1"), "t", "feature");
        t.saving_mode = true;
        t.autopilot = autopilot.map(|state| Autopilot { state, ..running() });
        t
    }

    #[test]
    fn shape_starts_the_implementation_only_while_running() {
        let offer = Some(Boundary::Offer(Step::Implement));
        let start = Some(Boundary::Start(Step::Implement));
        assert_eq!(
            shape(&task_with(Some(AutopilotState::Running)), offer),
            start
        );
        for state in [
            None,
            Some(AutopilotState::Paused),
            Some(AutopilotState::Done),
        ] {
            assert_eq!(shape(&task_with(state), offer), offer, "{state:?}");
        }
        let review = Some(Boundary::Start(Step::Review));
        assert_eq!(
            shape(&task_with(Some(AutopilotState::Running)), review),
            review
        );
        assert_eq!(shape(&task_with(Some(AutopilotState::Running)), None), None);
    }

    #[test]
    fn the_notes_follow_the_language() {
        let path = Path::new("/h/tasks/PROJ-1/autopilot/review-s1.json");
        assert!(step_note(Lang::En, Step::Plan, path).is_none());
        let es = step_note(Lang::Es, Step::Implement, path).unwrap();
        let en = step_note(Lang::En, Step::Implement, path).unwrap();
        assert!(es.contains("sin parar entre fases ni entre paquetes de trabajo"));
        assert!(en.contains("without stopping between phases or work packages"));
        let es = step_note(Lang::Es, Step::Review, path).unwrap();
        let en = step_note(Lang::En, Step::Review, path).unwrap();
        for (text, words) in [
            (
                &es,
                [
                    "checks",
                    "hallazgos confirmados",
                    "commit",
                    "título y la descripción",
                    "No hagas push ni abras una PR",
                ],
            ),
            (
                &en,
                [
                    "checks",
                    "confirmed findings",
                    "commit",
                    "title and description",
                    "Do not push or open a PR",
                ],
            ),
        ] {
            for word in words {
                assert!(text.contains(word), "{word}: {text}");
            }
            assert!(text.contains(&path.display().to_string()));
            assert!(text.contains("ready") && text.contains("blocked"));
        }
    }

    #[tokio::test]
    async fn the_outcome_file_lives_in_the_task_folder() {
        let (app, key, _dir) = task_with_workspace(Arc::new(FakeGh::new()), None).await;
        assert_eq!(
            outcome_path(&app, &key, "s1"),
            app.config
                .home
                .join("tasks/PROJ-1/autopilot/review-s1.json")
        );
    }

    fn file_with(content: Option<&str>) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("delilah-outcome-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("review-s1.json");
        if let Some(content) = content {
            std::fs::write(&path, content).unwrap();
        }
        (dir, path)
    }

    #[test]
    fn the_outcome_is_read_with_optional_fields() {
        let (_d, p) = file_with(Some(
            r#"{"outcome":"ready","pr_title":"feat: x","pr_body":"why"}"#,
        ));
        assert_eq!(
            read_outcome(Lang::En, &p),
            Outcome::Ready {
                pr_title: Some("feat: x".into()),
                pr_body: Some("why".into())
            }
        );
        let (_d, p) = file_with(Some(r#"{"outcome":"ready"}"#));
        assert_eq!(
            read_outcome(Lang::En, &p),
            Outcome::Ready {
                pr_title: None,
                pr_body: None
            }
        );
        let (_d, p) = file_with(Some(r#"{"outcome":"blocked","reason":"tests fail"}"#));
        assert_eq!(
            read_outcome(Lang::En, &p),
            Outcome::Blocked {
                reason: "tests fail".into()
            }
        );
    }

    #[test]
    fn a_missing_or_invalid_outcome_is_blocked_in_the_app_language() {
        for content in [None, Some("not json"), Some(r#"{"outcome":"maybe"}"#)] {
            let (_d, p) = file_with(content);
            assert_eq!(
                read_outcome(Lang::En, &p),
                Outcome::Blocked {
                    reason: "the review did not report an outcome".into()
                }
            );
            let Outcome::Blocked { reason } = read_outcome(Lang::Es, &p) else {
                panic!("not blocked");
            };
            assert!(reason.contains("la revisión no informó"));
        }
    }

    fn gh(list: &str) -> Arc<FakeGh> {
        Arc::new(
            FakeGh::new()
                .with_user("me")
                .on(&["repo", "view"], Ok(REPO_VIEW))
                .on(&["pr", "list"], Ok(list))
                .on(&["pr", "create"], Ok(PR_URL))
                .on(&["api", "graphql"], Ok(&bare_snapshot("OPEN", false))),
        )
    }

    /// A task on autopilot with a review session `rev-1`, whose branch has one commit
    /// beyond `main`.
    async fn ready_task(
        gh: Arc<FakeGh>,
        stage: Option<AutopilotStage>,
    ) -> (AppState, TaskKey, TempDir) {
        let (app, key, dir) = task_with_workspace(gh, None).await;
        git_repo_with_origin(
            &dir.path().join("ws"),
            &dir.path().join("origin"),
            "feature/PROJ-1",
            &["first"],
        );
        let a = Autopilot { stage, ..running() };
        app.store.set_autopilot(&key, Some(&a)).await.unwrap();
        app.store
            .create_session(&SessionRecord {
                id: "rev-1".into(),
                task: "PROJ-1".into(),
                kind: SessionKind::Claude,
                label: "Revisión".into(),
                primary: true,
                closed: false,
                read_only: false,
                conversation: false,
                step: Some(Step::Review),
            })
            .await
            .unwrap();
        (app, key, dir)
    }

    fn write_outcome(app: &AppState, key: &TaskKey, content: &str) {
        let path = outcome_path(app, key, "rev-1");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }

    async fn autopilot_of(app: &AppState, key: &TaskKey) -> Autopilot {
        app.store
            .get_task(key)
            .await
            .unwrap()
            .unwrap()
            .autopilot
            .unwrap()
    }

    fn creates(gh: &FakeGh) -> Vec<Vec<String>> {
        gh.calls()
            .into_iter()
            .filter(|c| c.starts_with(&["pr".to_string(), "create".to_string()]))
            .collect()
    }

    #[tokio::test]
    async fn a_ready_review_opens_one_draft_pr_with_the_agents_text_and_finishes() {
        let gh = gh("[]");
        let (app, key, _dir) = ready_task(gh.clone(), None).await;
        write_outcome(
            &app,
            &key,
            r#"{"outcome":"ready","pr_title":"feat: agent title","pr_body":"agent body"}"#,
        );
        finish(&app, &key, "rev-1").await;

        let made = creates(&gh);
        assert_eq!(made.len(), 1);
        assert!(made[0].contains(&"--draft".to_string()));
        assert!(made[0].contains(&"feat: agent title".to_string()));
        assert!(gh.stdins().iter().flatten().any(|s| s == "agent body"));
        let a = autopilot_of(&app, &key).await;
        assert_eq!((a.state, a.stage), (AutopilotState::Done, None));
        assert_eq!(app.store.prs_of(&key).await.unwrap().len(), 1);
        let events = app.store.events(&key).await.unwrap();
        let finished: Vec<_> = events
            .iter()
            .filter(|e| e.event.name() == "autopilot-finished")
            .collect();
        assert_eq!(finished.len(), 1);
        assert_eq!(
            finished[0].event,
            Event::AutopilotFinished {
                prs: vec![PR_URL.into()]
            }
        );

        // Finishing again opens nothing.
        finish(&app, &key, "rev-1").await;
        assert_eq!(creates(&gh).len(), 1);
    }

    #[tokio::test]
    async fn the_stage_is_opening_pr_while_the_prs_are_opened() {
        // `gh pr create` fails: the PR is not opened and the stage stays recorded.
        let failing = Arc::new(
            FakeGh::new()
                .with_user("me")
                .on(&["repo", "view"], Ok(REPO_VIEW))
                .on(&["pr", "list"], Ok("[]"))
                .on(
                    &["pr", "create"],
                    Err(crate::gh::GhError::Failed {
                        code: Some(1),
                        stderr: "boom".into(),
                    }),
                ),
        );
        let (app, key, _dir) = ready_task(failing, None).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        finish(&app, &key, "rev-1").await;
        let a = autopilot_of(&app, &key).await;
        assert_eq!(
            (a.state, a.stage),
            (AutopilotState::Running, Some(AutopilotStage::OpeningPr))
        );
    }

    #[tokio::test]
    async fn without_title_or_body_the_pr_uses_the_proposal() {
        let gh = gh("[]");
        let (app, key, _dir) = ready_task(gh.clone(), None).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        finish(&app, &key, "rev-1").await;
        let made = creates(&gh);
        assert_eq!(made.len(), 1);
        assert!(made[0].contains(&"t".to_string()), "the task's title");
        assert!(gh.stdins().iter().flatten().any(|s| s.contains("- first")));
    }

    #[tokio::test]
    async fn a_blocked_review_opens_nothing() {
        let gh = gh("[]");
        let (app, key, _dir) = ready_task(gh.clone(), None).await;
        write_outcome(&app, &key, r#"{"outcome":"blocked","reason":"tests fail"}"#);
        finish(&app, &key, "rev-1").await;
        assert!(creates(&gh).is_empty());
        assert_eq!(
            autopilot_of(&app, &key).await.state,
            AutopilotState::Running
        );
    }

    #[tokio::test]
    async fn recovery_links_the_pr_gh_already_has_and_never_creates_one() {
        let gh = gh(PR_LIST);
        let (app, key, _dir) = ready_task(gh.clone(), Some(AutopilotStage::OpeningPr)).await;
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover(&app, &task).await;
        assert!(creates(&gh).is_empty());
        let prs = app.store.prs_of(&key).await.unwrap();
        assert_eq!(prs.len(), 1);
        assert_eq!(autopilot_of(&app, &key).await.state, AutopilotState::Done);
    }

    async fn with_preview(app: &mut AppState, command: &str, timeout_ms: u64) {
        app.store
            .set_preview_command("proj", Some(command))
            .await
            .unwrap();
        app.previews = Arc::new(crate::preview::Previews::with_timeout(
            crate::events::Events::default(),
            std::time::Duration::from_millis(timeout_ms),
        ));
    }

    #[tokio::test]
    async fn a_preview_that_runs_puts_its_url_in_the_pr_body() {
        let gh = gh("[]");
        let (mut app, key, _dir) = ready_task(gh.clone(), None).await;
        with_preview(&mut app, "echo http://127.0.0.1:9/x; sleep 30", 5000).await;
        write_outcome(&app, &key, r#"{"outcome":"ready","pr_body":"agent body"}"#);
        finish(&app, &key, "rev-1").await;
        assert_eq!(creates(&gh).len(), 1);
        assert!(
            gh.stdins()
                .iter()
                .flatten()
                .any(|s| s.contains("agent body") && s.contains("Preview: http://127.0.0.1:9/x")),
            "{:?}",
            gh.stdins()
        );
        assert_eq!(autopilot_of(&app, &key).await.state, AutopilotState::Done);
        app.previews.stop(key.as_str());
    }

    #[tokio::test]
    async fn a_failed_preview_still_opens_the_pr_and_does_not_pause() {
        let gh = gh("[]");
        let (mut app, key, _dir) = ready_task(gh.clone(), None).await;
        with_preview(&mut app, "echo oops; exit 3", 5000).await;
        write_outcome(&app, &key, r#"{"outcome":"ready","pr_body":"agent body"}"#);
        finish(&app, &key, "rev-1").await;
        assert_eq!(creates(&gh).len(), 1);
        assert!(!gh.stdins().iter().flatten().any(|s| s.contains("Preview:")));
        assert_eq!(autopilot_of(&app, &key).await.state, AutopilotState::Done);
    }

    #[tokio::test]
    async fn a_preview_that_prints_no_url_times_out_and_the_pr_opens() {
        let gh = gh("[]");
        let (mut app, key, _dir) = ready_task(gh.clone(), None).await;
        with_preview(&mut app, "sleep 30", 300).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        finish(&app, &key, "rev-1").await;
        assert_eq!(creates(&gh).len(), 1);
        assert_eq!(autopilot_of(&app, &key).await.state, AutopilotState::Done);
        app.previews.stop(key.as_str());
    }

    #[tokio::test]
    async fn the_stage_is_preview_while_the_preview_starts() {
        let gh = gh("[]");
        let (mut app, key, _dir) = ready_task(gh.clone(), None).await;
        with_preview(&mut app, "sleep 30", 60_000).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        let a2 = app.clone();
        let k2 = key.clone();
        let handle = tokio::spawn(async move { finish(&a2, &k2, "rev-1").await });
        for _ in 0..100 {
            if autopilot_of(&app, &key).await.stage == Some(AutopilotStage::Preview) {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert_eq!(
            autopilot_of(&app, &key).await.stage,
            Some(AutopilotStage::Preview)
        );
        assert!(creates(&gh).is_empty());
        // Recovery leaves it alone while the preview is starting.
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover(&app, &task).await;
        assert!(creates(&gh).is_empty());
        handle.abort();
        app.previews.stop(key.as_str());
    }

    #[tokio::test]
    async fn recovery_of_a_lost_preview_goes_on_to_the_pr() {
        let gh = gh("[]");
        let (mut app, key, _dir) = ready_task(gh.clone(), Some(AutopilotStage::Preview)).await;
        with_preview(&mut app, "echo oops; exit 3", 5000).await;
        write_outcome(&app, &key, r#"{"outcome":"ready"}"#);
        let task = app.store.get_task(&key).await.unwrap().unwrap();
        recover(&app, &task).await;
        assert_eq!(creates(&gh).len(), 1);
        assert_eq!(autopilot_of(&app, &key).await.state, AutopilotState::Done);
    }
}
