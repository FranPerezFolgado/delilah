//! The always-running process: scheduler, hook server and API.

mod actions;
pub mod api;
pub mod config;
mod hooks;
pub mod notify;
mod projects;
mod run;
pub mod scheduler;
pub mod sessions;
pub mod state;
pub mod store;
mod store_projects;
mod store_runs;
mod terminal;
mod transitions;
