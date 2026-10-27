//! HTTP handlers for parent tasks and subtasks.

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    use crate::api::router;
    use crate::state::AppState;
    use crate::store::Store;

    async fn call(
        app: &Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(json) => {
                req = req.header("content-type", "application/json");
                Body::from(json.to_string())
            }
            None => Body::empty(),
        };
        let res = app.clone().oneshot(req.body(body).unwrap()).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, value)
    }

    async fn app() -> (Router, AppState) {
        let config = crate::config::Config {
            home: std::env::temp_dir().join(format!("delilah-api-{}", uuid::Uuid::new_v4())),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        let state = AppState::new(Store::in_memory().await.unwrap(), config);
        for name in ["web", "api"] {
            state
                .store
                .create_project(&delilah_core::Project {
                    name: name.into(),
                    path: format!("/w/{name}").into(),
                    kind: delilah_core::ProjectKind::Repo,
                    workflow: "prompt".into(),
                    settings: crate::autopilot::feature_settings(),
                    preview_command: None,
                })
                .await
                .unwrap();
        }
        (router(state.clone()), state)
    }

    async fn create(app: &Router, body: Value) -> (StatusCode, Value) {
        call(app, "POST", "/api/v1/tasks", Some(body)).await
    }

    async fn parent(app: &Router) -> Value {
        let (s, p) = create(
            app,
            json!({"free": true, "title": "P", "project": "web", "kind": "parent_task",
                   "saving_mode": true, "autopilot": false, "ci_autofix": true}),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{p}");
        p
    }

    #[tokio::test]
    async fn a_parent_task_has_no_agent_settings() {
        let (app, _) = app().await;
        let p = parent(&app).await;
        assert_eq!(p["kind"], "parent_task");
        assert_eq!(p["workflow"], "none");
        assert_eq!(p["saving_mode"], false);
        assert!(p["autopilot"].is_null());
        assert_eq!(p["ci_autofix"], false);
        assert_ne!(p["in_checkout"], true);
    }

    #[tokio::test]
    async fn a_subtask_lands_in_the_parent_tasks_project_with_free_keys() {
        let (app, _) = app().await;
        let p = parent(&app).await;
        let pk = p["key"].as_str().unwrap();
        let (s, a) = create(&app, json!({"free": true, "title": "a", "parent_task": pk})).await;
        assert_eq!(s, StatusCode::CREATED, "{a}");
        assert_eq!(a["kind"], "subtask");
        assert_eq!(a["parent_task"], pk);
        assert_eq!(a["project"], "web");
        let (s, b) = create(
            &app,
            json!({"free": true, "title": "b", "parent_task": pk, "project": "web"}),
        )
        .await;
        assert_eq!(s, StatusCode::CREATED, "{b}");
        assert_ne!(a["key"], b["key"]);
        assert_ne!(a["key"], p["key"]);
    }

    #[tokio::test]
    async fn creating_a_subtask_is_refused_with_the_contract_reason() {
        let (app, state) = app().await;
        let p = parent(&app).await;
        let pk = p["key"].as_str().unwrap().to_string();
        let (_, normal) = create(&app, json!({"free": true, "title": "n", "project": "web"})).await;
        let (_, sub) = create(&app, json!({"free": true, "title": "s", "parent_task": pk})).await;

        let (s, body) = create(
            &app,
            json!({"free": true, "title": "x", "parent_task": normal["key"]}),
        )
        .await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::CONFLICT, Some("not-a-parent-task")),
            "{body}"
        );
        assert!(body["message"].is_string());
        let (s, body) = create(
            &app,
            json!({"free": true, "title": "x", "parent_task": sub["key"]}),
        )
        .await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::CONFLICT, Some("not-a-parent-task"))
        );
        let (s, body) = create(
            &app,
            json!({"free": true, "title": "x", "parent_task": "NOPE-1"}),
        )
        .await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::CONFLICT, Some("not-a-parent-task"))
        );
        let (s, body) = create(
            &app,
            json!({"free": true, "title": "x", "kind": "parent_task", "parent_task": pk}),
        )
        .await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::CONFLICT, Some("nesting")),
            "{body}"
        );
        let (s, body) = create(
            &app,
            json!({"free": true, "title": "x", "parent_task": pk, "project": "api"}),
        )
        .await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::CONFLICT, Some("project-mismatch")),
            "{body}"
        );

        sqlx::query("UPDATE tasks SET status = '{\"type\":\"done\"}' WHERE key = ?")
            .bind(&pk)
            .execute(&state.store.pool)
            .await
            .unwrap();
        let (s, body) = create(&app, json!({"free": true, "title": "x", "parent_task": pk})).await;
        assert_eq!(
            (s, body["error"].as_str()),
            (StatusCode::CONFLICT, Some("parent-task-archived")),
            "{body}"
        );
    }

    #[tokio::test]
    async fn tasks_carry_kind_parent_and_progress() {
        let (app, _) = app().await;
        let p = parent(&app).await;
        let pk = p["key"].as_str().unwrap();
        create(&app, json!({"free": true, "title": "a", "parent_task": pk})).await;
        create(&app, json!({"free": true, "title": "b", "parent_task": pk})).await;
        let (_, list) = call(&app, "GET", "/api/v1/tasks", None).await;
        let list = list.as_array().unwrap();
        let by_key = |k: &str| list.iter().find(|t| t["key"] == k).unwrap().clone();
        let got = by_key(pk);
        assert_eq!(
            got["subtask_progress"],
            json!({"merged": 0, "counted": 2, "dropped": 0, "ready": false})
        );
        let sub = list.iter().find(|t| t["kind"] == "subtask").unwrap();
        assert_eq!(sub["parent_task"], pk);
        assert!(sub.get("subtask_progress").is_none());
        let (_, one) = call(&app, "GET", &format!("/api/v1/tasks/{pk}"), None).await;
        assert_eq!(one["subtask_progress"]["counted"], 2);
        assert_eq!(one["kind"], "parent_task");
    }
}
