//! A fake Jira Cloud on 127.0.0.1, answering the routes `delilah_integrations::jira` calls
//! from the sample fixture. No real credentials are involved (FR-009).

use axum::extract::Path;
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde_json::{Value, json};

use crate::env::JiraHandle;

const FIXTURE: &str = include_str!("../../delilah-integrations/fixtures/jira-search.json");
const SPRINT_SCHEMA: &str = "com.pyxis.greenhopper.jira:gh-sprint";

fn issues() -> Vec<Value> {
    let v: Value = serde_json::from_str(FIXTURE).expect("jira-search.json");
    v["issues"].as_array().cloned().unwrap_or_default()
}

fn router() -> Router {
    Router::new()
        .route("/rest/api/3/myself", get(|| async { Json(json!({"displayName": "Fran"})) }))
        .route(
            "/rest/api/3/field",
            get(|| async {
                Json(json!([{"id": "customfield_10020", "name": "Sprint",
                    "schema": {"custom": SPRINT_SCHEMA}}]))
            }),
        )
        .route(
            "/rest/api/3/search/jql",
            get(|| async { Json(json!({"issues": issues(), "isLast": true})) }),
        )
        .route(
            "/rest/api/3/issue/{key}",
            get(|Path(key): Path<String>| async move {
                match issues().into_iter().find(|i| i["key"] == key.as_str()) {
                    Some(i) => Ok(Json(i)),
                    None => Err(StatusCode::NOT_FOUND),
                }
            }),
        )
}

/// Starts the server on a free loopback port in its own thread; the handle's `stop`
/// shuts it down and joins the thread.
pub fn start() -> JiraHandle {
    let (tx_url, rx_url) = std::sync::mpsc::channel();
    let (tx_stop, rx_stop) = tokio::sync::oneshot::channel::<()>();
    let thread = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("jira runtime");
        rt.block_on(async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
            let port = listener.local_addr().expect("addr").port();
            tx_url.send(format!("http://127.0.0.1:{port}")).expect("url");
            let _ = axum::serve(listener, router())
                .with_graceful_shutdown(async {
                    let _ = rx_stop.await;
                })
                .await;
        });
    });
    let url = rx_url.recv().expect("jira server url");
    JiraHandle {
        url,
        stop: Some(Box::new(move || {
            let _ = tx_stop.send(());
            let _ = thread.join();
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use delilah_integrations::jira::{Filter, JiraClient};

    #[test]
    fn the_real_client_reads_the_fixture_issues() {
        let mut jira = start();
        let url = jira.url.clone();
        let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        let client = JiraClient::cloud(&url, "e2e@example.invalid", "not-a-token");
        let (me, listed, one) = rt.block_on(async {
            (
                client.myself().await.unwrap(),
                client.list(&Filter::default(), "").await.unwrap(),
                client.issue("PROJ-1242").await.unwrap(),
            )
        });
        assert_eq!(me, "Fran");
        assert_eq!(listed.len(), issues().len());
        assert!(listed.iter().any(|i| i.key == "PROJ-1242"));
        assert_eq!(one.key, "PROJ-1242");
        assert!(one.sprint.is_some(), "the sprint field is wired");
        (jira.stop.take().unwrap())();
        assert!(std::net::TcpStream::connect(url.trim_start_matches("http://")).is_err());
    }
}
