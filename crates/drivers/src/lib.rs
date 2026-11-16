//! Database drivers and the query engine (SPEC §1.3–1.5).
//!
//! - [`pool`]: per-connection pools and connect options
//! - [`introspect`]: schema browsing queries
//! - [`exec`]: statement execution that streams [`QueryEvent`]s in columnar batches
//! - [`tasks`]: cancellable task registry with server-side cancel
//! - [`manager`]: live connections + pinned editor sessions

pub mod convert;
pub mod error;
pub mod exec;
pub mod introspect;
pub mod manager;
pub mod pool;
pub mod tasks;

pub use exec::{ExecOptions, execute_statements};
pub use manager::{ConnectionManager, LiveConnection, Session};
pub use pool::{ConnectParams, DbPool};
pub use snaptable_core::query::QueryEvent;
pub use tasks::TaskRegistry;
