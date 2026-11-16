//! Shared domain types for SnapTable.
//!
//! Everything that crosses the IPC boundary lives here and derives [`ts_rs::TS`],
//! so the TypeScript bindings in `apps/desktop/ui/src/ipc/bindings` are generated
//! from these definitions (`cargo test -p snaptable-core` regenerates them).

pub mod error;
pub mod model;
pub mod query;
pub mod time;

pub use error::{AppError, AppResult};
