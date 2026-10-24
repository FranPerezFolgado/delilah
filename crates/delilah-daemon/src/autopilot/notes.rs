//! What the sessions of an autopilot task are told, and where the review writes its
//! outcome.

use std::path::{Path, PathBuf};

use delilah_core::{Step, TaskKey};

use super::{is_running, update};
use crate::lang::Lang;
use crate::state::AppState;

/// Where the review session `session` writes its outcome.
pub(crate) fn outcome_path(app: &AppState, key: &TaskKey, session: &str) -> PathBuf {
    app.config
        .task_dir(key.as_str())
        .join("autopilot")
        .join(format!("review-{session}.json"))
}

/// What the first session of an autopilot task is told on top of its prompt: run the
/// spec workflow end to end on its own, then stop once `tasks.md` is written.
pub(crate) fn autopilot_plan_note(lang: Lang, key: &str) -> String {
    match lang {
        Lang::Es => format!(
            "Autopilot ({key}): nadie va a responder ni aprobar nada. Contesta tú las preguntas de configuración del workflow: salida en markdown, usa la rama actual de la tarea, sin fase de clarificación y con tareas generadas, solo spec y plan sin implementar. Aprueba tú cada fase y no preguntes nada que tenga una opción recomendada; si tienes que preguntar, marca una como \"(Recomendado)\". Cuando `tasks.md` esté escrito, para: Delilah seguirá en una sesión nueva."
        ),
        Lang::En => format!(
            "Autopilot ({key}): nobody will answer or approve anything. Answer the workflow's setup questions yourself: markdown output, keep the task's current branch, no clarification phase, and generate the tasks, spec and plan only with no implementation. Approve every phase yourself and do not ask anything that has a recommended option; if you must ask, mark one \"(Recommended)\". Once `tasks.md` is written, stop: Delilah will carry on in a new session."
        ),
    }
}

/// What a step's session is told on top of its prompt when the task is on autopilot.
/// `outcome` is where the review writes its outcome. The plan step has its own note
/// (`autopilot_plan_note`).
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

pub(super) fn tasks_unchecked(lang: Lang, n: usize) -> String {
    match (lang, n) {
        (Lang::Es, 1) => "1 tarea sin marcar".into(),
        (Lang::Es, n) => format!("{n} tareas sin marcar"),
        (Lang::En, 1) => "1 task unchecked".into(),
        (Lang::En, n) => format!("{n} tasks unchecked"),
    }
}

/// What a resumed review is told when its outcome still says blocked: the user may have
/// fixed it by hand, so it checks again and writes the outcome anew.
pub(crate) fn recheck_text(lang: Lang, outcome: &Path) -> String {
    let path = outcome.display();
    match lang {
        Lang::Es => format!(
            "Autopilot reanudado: vuelve a comprobar la revisión (puede que ya se haya resuelto lo que la bloqueaba), haz commit de lo que falte y escribe de nuevo el resultado en `{path}`."
        ),
        Lang::En => format!(
            "Autopilot resumed: check the review again (what blocked it may be solved by now), commit what is missing and write the outcome to `{path}` again."
        ),
    }
}

pub(crate) fn nudge_text(lang: Lang) -> &'static str {
    lang.pick(
        "Autopilot: sigue adelante sin preguntar; elige tú la opción recomendada; si de verdad no puedes continuar, di por qué y para.",
        "Autopilot: carry on without asking; pick the recommended option yourself; if you truly cannot go on, say why and stop.",
    )
}

/// What the step's session is told on top of its prompt, if the task is on autopilot.
/// For the review it also makes the folder of its outcome file and records the
/// session, so a restart reads that session's file. `session` is the one that runs.
pub(crate) async fn prepare_step(
    app: &AppState,
    key: &TaskKey,
    step: Step,
    session: &str,
) -> Option<String> {
    let Ok(Some(task)) = app.store.get_task(key).await else {
        return None;
    };
    if !is_running(&task) {
        return None;
    }
    let path = outcome_path(app, key, session);
    if step == Step::Review {
        if let Some(dir) = path.parent()
            && let Err(e) = std::fs::create_dir_all(dir)
        {
            tracing::warn!(task = %key, "cannot create {}: {e}", dir.display());
        }
        let recorded = update(app, key, |a| {
            let running = a.state == delilah_core::AutopilotState::Running;
            if running {
                a.review_session = Some(session.to_string());
            }
            running
        })
        .await;
        if let Err(e) = recorded {
            tracing::warn!(task = %key, "cannot record the autopilot's review session: {e}");
        }
    }
    step_note(app.lang(), step, &path)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::sync::Arc;

    use delilah_core::Step;

    use crate::gh::fake::FakeGh;

    use crate::lang::Lang;
    use crate::pr_testkit::task_with_workspace;

    use super::*;
    use crate::autopilot::testkit::*;

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

    #[tokio::test]
    async fn preparing_the_review_makes_its_folder_and_records_the_session() {
        let (app, key, _dir) = ready_task(gh("[]"), None).await;
        let path = outcome_path(&app, &key, "rev-2");
        assert!(!path.parent().unwrap().exists());
        let note = prepare_step(&app, &key, Step::Review, "rev-2")
            .await
            .unwrap();
        assert!(note.contains(&path.display().to_string()));
        assert!(path.parent().unwrap().is_dir());
        assert_eq!(
            autopilot_of(&app, &key).await.review_session.as_deref(),
            Some("rev-2")
        );
        // Not on autopilot: nothing.
        app.store.set_autopilot(&key, None).await.unwrap();
        assert!(
            prepare_step(&app, &key, Step::Review, "rev-3")
                .await
                .is_none()
        );
    }
}
