//! The always-running process: scheduler, hook server and API.

mod actions;
pub mod api;
pub mod asks;
mod changes;
pub mod config;
pub mod events;
mod files;
// Called by the hook, routes and scheduler that arrive with the next package.
#[allow(dead_code)]
mod handoff;
mod hooks;
pub mod inbox;
mod jira;
pub mod lang;
pub mod logs;
pub mod notify;
mod owners;
pub mod phases;
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
