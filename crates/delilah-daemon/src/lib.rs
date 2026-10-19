//! The always-running process: scheduler, hook server and API.

mod actions;
pub mod api;
pub mod config;
mod hooks;
mod jira;
pub mod notify;
mod projects;
mod run;
pub mod scheduler;
pub mod secrets;
mod session_api;
pub mod sessions;
pub mod state;
pub mod store;
mod store_projects;
mod store_runs;
pub mod store_sessions;
mod store_settings;
mod terminal;
mod transitions;
