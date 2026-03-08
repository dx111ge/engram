//! engram-a2a: Agent-to-Agent protocol implementation.
//!
//! Provides A2A interoperability for engram as an AI agent:
//! - [`skill`] -- skill registration and capability advertisement
//! - [`task`] -- task lifecycle management (pending, running, completed)
//! - [`card`] -- agent card (identity, capabilities, endpoints)
//! - [`discovery`] -- agent discovery and registration
//! - [`streaming`] -- streaming task updates via SSE
//! - [`notification`] -- push notifications for task state changes

pub mod card;
pub mod discovery;
pub mod notification;
pub mod skill;
pub mod streaming;
pub mod task;
