//! The always-running process: scheduler, hook server and API.

mod actions;
pub mod api;
mod changes;
pub mod config;
mod files;
mod hooks;
mod jira;
pub mod notify;
mod preferences;
mod projects;
mod run;
pub mod scheduler;
pub mod secrets;
mod session_api;
pub mod sessions;
pub mod specs;
pub mod state;
pub mod store;
mod store_projects;
mod store_runs;
pub mod store_sessions;
mod store_settings;
mod terminal;
mod transitions;
