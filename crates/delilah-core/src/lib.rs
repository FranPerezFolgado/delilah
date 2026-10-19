//! Task model and state machine. Pure logic, no I/O.

mod task;

pub use task::{Event, Task, TaskKey, TaskStatus, TransitionError};
