use rusqlite::Connection;
use std::sync::{Arc, Mutex};

mod branches;
mod conversations;
mod custom_backends;
mod messages;
mod models;
mod system_prompts;
mod usage;

pub use branches::*;
pub use conversations::*;
pub use custom_backends::*;
pub use messages::*;
pub use models::*;
pub use system_prompts::*;
pub use usage::*;

pub trait DbOperations {
    fn conn(&self) -> Arc<Mutex<Connection>>;
} 