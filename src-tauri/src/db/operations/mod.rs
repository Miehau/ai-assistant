use rusqlite::Connection;
use std::sync::{Arc, Mutex};

mod branches;
mod conversations;
mod messages;
mod models;
mod system_prompts;
mod usage;

pub use branches::*;
pub use conversations::*;
pub use messages::*;
pub use models::*;
pub use system_prompts::*;
pub use usage::*;

pub trait DbOperations {
    fn conn(&self) -> Arc<Mutex<Connection>>;
} 