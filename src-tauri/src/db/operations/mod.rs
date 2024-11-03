use rusqlite::{Connection, Result as RusqliteResult};
use std::sync::{Arc, Mutex};
use crate::db::models::*;

mod messages;
mod conversations;
mod models;
mod system_prompts;

pub use messages::*;
pub use conversations::*;
pub use models::*;
pub use system_prompts::*;

pub trait DbOperations {
    fn conn(&self) -> Arc<Mutex<Connection>>;
} 