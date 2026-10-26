//! Task model and state machine. Pure logic, no I/O.

pub mod branch;
mod project;
mod spec;
mod task;
pub mod workflow;

pub use project::{
    FieldError, Project, ProjectKind, ProjectSettings, ShareMode, SharedEntry, SubmoduleRules,
    WorkspaceMode, branch_name, slug, validate,
};
pub use spec::{SpecGroup, SpecItem, SpecPhase, parse_tasks};
pub use task::{
    Autopilot, AutopilotStage, AutopilotState, Event, Handoff, HandoffStage, InvalidKey, Step,
    Task, TaskKey, TaskStatus, TaskView, TitleSource, TransitionError,
};
pub use workflow::{
    ProjectWorkflow, ResolvedStart, Start, StartRequest, TaskWorkflow, Vars, is_phased, render,
    resolve,
};
