//! Task model and state machine. Pure logic, no I/O.

mod project;
mod task;

pub use project::{Project, ProjectKind};
pub use task::{Event, Task, TaskKey, TaskStatus, TransitionError};
