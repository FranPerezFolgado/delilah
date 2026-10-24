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
    app.store.set_setting(&setting_key(&key)?, &value).await?;
    if key == "language" {
        app.load_language().await;
    }
    if key == "remote-control" {
        app.load_remote_control().await;
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
