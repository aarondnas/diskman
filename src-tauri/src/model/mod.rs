//! Domain model: the data Diskman works with and its rules, without any storage or UI.

pub mod config;

pub use config::{Backup, Config, Mode, Schedule};
