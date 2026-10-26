//! `/api/v1/preferences/{key}`: small UI preferences (e.g. keyboard shortcuts) kept by
//! the daemon, so every window and device sees the same ones.

use axum::Json;
use axum::extract::{Path, State};
use serde_json::Value;

use crate::api::ApiError;
use crate::state::AppState;

fn setting_key(key: &str) -> Result<String, ApiError> {
    let valid = !key.is_empty()
        && key.len() <= 40
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        return Err(ApiError::bad_request(format!(
            "`{key}` is not a valid preference name"
        )));
    }
    // Namespaced, so a preference can never overwrite e.g. the Jira settings.
    Ok(format!("pref:{key}"))
}

pub async fn get(
    State(app): State<AppState>,
    Path(key): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let value: Option<Value> = app.store.setting(&setting_key(&key)?).await?;
    Ok(Json(value.unwrap_or(Value::Null)))
}

pub async fn put(
    State(app): State<AppState>,
    Path(key): Path<String>,
    Json(value): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if value.to_string().len() > 64 * 1024 {
        return Err(ApiError::bad_request("preference too large"));
    }
    // Checked before saving, so a bad value never reaches the store.
    if key == "agent-permissions" && !matches!(value.as_str(), Some("auto" | "default")) {
        return Err(ApiError::bad_request(
            "`agent-permissions` must be \"auto\" or \"default\"",
        ));
    }
    if key == crate::models::MODELS_PREF {
        crate::models::validate(&value)?;
    }
    if key == crate::models::SUGGEST_PREF && !value.is_boolean() {
        return Err(ApiError::bad_request(
            "`model-suggestion` must be true or false",
        ));
    }
    app.store.set_setting(&setting_key(&key)?, &value).await?;
    match key.as_str() {
        "language" => app.load_language().await,
        "remote-control" => app.load_remote_control().await,
        "agent-permissions" => app.load_agent_permissions().await,
        _ => {}
    }
    Ok(Json(value))
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serde_json::json;
    use tower::ServiceExt;

    use super::*;
    use crate::config::Config;
    use crate::store::Store;

    async fn call(
        app: &AppState,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method).uri(uri);
        let body = match body {
            Some(b) => {
                req = req.header("content-type", "application/json");
                Body::from(b.to_string())
            }
            None => Body::empty(),
        };
        let res = crate::api::router(app.clone())
            .oneshot(req.body(body).unwrap())
            .await
            .unwrap();
        let status = res.status();
        let bytes = http_body_util::BodyExt::collect(res.into_body())
            .await
            .unwrap()
            .to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    async fn app() -> AppState {
        let config = Config {
            home: std::env::temp_dir(),
            addr: crate::config::DEFAULT_ADDR.parse().unwrap(),
            allow_file_protocol: false,
            notify: false,
            ui_origin: None,
            claude_bin: "claude".into(),
            agent_program: None,
            ..crate::config::Config::test_defaults()
        };
        AppState::new(Store::in_memory().await.unwrap(), config)
    }

    #[tokio::test]
    async fn keeps_preferences_apart_from_other_settings() {
        let app = app().await;
        let (_, empty) = call(&app, "GET", "/api/v1/preferences/shortcuts", None).await;
        assert_eq!(empty, Value::Null);

        let shortcuts = json!({ "new-terminal": "Mod+T" });
        call(
            &app,
            "PUT",
            "/api/v1/preferences/shortcuts",
            Some(shortcuts.clone()),
        )
        .await;
        let (_, saved) = call(&app, "GET", "/api/v1/preferences/shortcuts", None).await;
        assert_eq!(saved, shortcuts);

        let (code, _) = call(&app, "PUT", "/api/v1/preferences/Bad%20Key", Some(json!(1))).await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
        // `jira` the preference is not `jira` the connection.
        call(&app, "PUT", "/api/v1/preferences/jira", Some(json!("x"))).await;
        let (_, jira) = call(&app, "GET", "/api/v1/jira", None).await;
        assert_eq!(jira["settings"], Value::Null);
    }

    #[tokio::test]
    async fn the_remote_control_preference_takes_effect_at_once() {
        let app = app().await;
        assert!(!app.remote_control());
        let uri = "/api/v1/preferences/remote-control";
        call(&app, "PUT", uri, Some(json!(true))).await;
        assert!(app.remote_control());
        call(&app, "PUT", uri, Some(json!(false))).await;
        assert!(!app.remote_control());
        // Only a real `true` turns it on.
        call(&app, "PUT", uri, Some(json!("yes"))).await;
        assert!(!app.remote_control());
    }

    #[tokio::test]
    async fn the_agent_permissions_preference_takes_effect_at_once() {
        use crate::state::AgentPermissions;
        let app = app().await;
        assert_eq!(app.agent_permissions(), AgentPermissions::Auto);
        let uri = "/api/v1/preferences/agent-permissions";
        let (code, _) = call(&app, "PUT", uri, Some(json!("default"))).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(app.agent_permissions(), AgentPermissions::Default);
        call(&app, "PUT", uri, Some(json!("auto"))).await;
        assert_eq!(app.agent_permissions(), AgentPermissions::Auto);
    }

    #[tokio::test]
    async fn a_bad_agent_permissions_value_is_a_400_and_changes_nothing() {
        use crate::state::AgentPermissions;
        let app = app().await;
        let uri = "/api/v1/preferences/agent-permissions";
        call(&app, "PUT", uri, Some(json!("default"))).await;
        for bad in [json!("bypassPermissions"), json!(1), json!(null), json!({})] {
            let (code, _) = call(&app, "PUT", uri, Some(bad.clone())).await;
            assert_eq!(code, StatusCode::BAD_REQUEST, "{bad}");
            let (_, saved) = call(&app, "GET", uri, None).await;
            assert_eq!(saved, json!("default"), "{bad}");
            assert_eq!(app.agent_permissions(), AgentPermissions::Default);
        }
    }

    #[tokio::test]
    async fn the_agent_models_are_checked_before_saving() {
        let app = app().await;
        let uri = "/api/v1/preferences/agent-models";
        let good = json!({"specify": "opus", "plan": "opusplan", "implement": "haiku", "review": "default"});
        let (code, _) = call(&app, "PUT", uri, Some(good.clone())).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(
            crate::models::phase_models(&app).await.implement,
            delilah_core::AgentModel::Haiku
        );
        for bad in [
            json!({"specify": "opus"}),
            json!({"specify": "gpt-5", "plan": "opus", "implement": "opus", "review": "opus"}),
            json!("sonnet"),
        ] {
            let (code, _) = call(&app, "PUT", uri, Some(bad.clone())).await;
            assert_eq!(code, StatusCode::BAD_REQUEST, "{bad}");
            let (_, saved) = call(&app, "GET", uri, None).await;
            assert_eq!(saved, good, "{bad}");
        }
        // `null` is the defaults again.
        let (code, _) = call(&app, "PUT", uri, Some(json!(null))).await;
        assert_eq!(code, StatusCode::OK);
        assert_eq!(
            crate::models::phase_models(&app).await,
            delilah_core::PhaseModels::default()
        );
        let (code, _) = call(
            &app,
            "PUT",
            "/api/v1/preferences/model-suggestion",
            Some(json!("on")),
        )
        .await;
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn other_preferences_still_accept_any_json() {
        let app = app().await;
        for v in [json!(1), json!(null), json!({"a": [1]}), json!("x")] {
            let (code, _) = call(&app, "PUT", "/api/v1/preferences/whatever", Some(v)).await;
            assert_eq!(code, StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn the_remote_control_preference_is_read_at_start() {
        let app = app().await;
        app.store
            .set_setting("pref:remote-control", &json!(true))
            .await
            .unwrap();
        assert!(!app.remote_control());
        app.load_remote_control().await;
        assert!(app.remote_control());
    }
}
