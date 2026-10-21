//! Task model and state machine. Pure logic, no I/O.

mod project;
mod spec;
mod task;

pub use project::{
    DEFAULT_BRANCH_TEMPLATE, FieldError, Project, ProjectKind, ProjectSettings, ShareMode,
    SharedEntry, SubmoduleRules, WorkspaceMode, validate,
};
pub use spec::{SpecGroup, SpecItem, SpecPhase, parse_tasks};
pub use task::{Event, InvalidKey, Task, TaskKey, TaskStatus, TransitionError};
