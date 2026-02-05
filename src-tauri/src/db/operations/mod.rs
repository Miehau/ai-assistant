use rusqlite::Connection;
use std::sync::{Arc, Mutex};

mod agent_sessions;
mod branches;
mod conversations;
mod custom_backends;
mod integration_connections;
mod mcp_servers;
mod messages;
mod models;
mod preferences;
mod system_prompts;
mod usage;

pub use agent_sessions::*;
pub use branches::*;
pub use conversations::*;
pub use custom_backends::*;
pub use integration_connections::*;
pub use mcp_servers::*;
pub use messages::*;
pub use models::*;
pub use preferences::*;
pub use system_prompts::*;
pub use usage::*;

pub trait DbOperations {
    fn conn(&self) -> Arc<Mutex<Connection>>;
}
