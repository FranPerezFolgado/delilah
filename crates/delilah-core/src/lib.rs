//! Task model and state machine. Pure logic, no I/O.

pub mod branch;
mod project;
mod spec;
mod task;

pub use project::{Project, ProjectKind};
pub use spec::{SpecGroup, SpecItem, SpecPhase, parse_tasks};
pub use task::{
    Autopilot, AutopilotStage, AutopilotState, Event, Handoff, HandoffStage, InvalidKey, Step,
    Task, TaskKey, TaskStatus, TaskView, TransitionError,
};
