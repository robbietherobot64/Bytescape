//! Tauri Command Modules
//! 
//! This module organizes Tauri commands by concern area for better maintainability.
//! Each submodule exports its commands which are re-exported here for registration.

pub mod node;
pub mod vault;
pub mod auth;
pub mod import;
pub mod media;
pub mod feed_aggregation;
pub mod syndicate;
pub mod posting;
pub mod interactions;
pub mod peers;
pub mod collab;
pub mod ap;
pub mod profile;
pub mod unfurl;
pub mod classify;
pub mod export;
pub mod browser;

// Re-export all commands for easy registration in lib.rs
pub use node::*;
pub use vault::*;
pub use auth::*;
pub use import::*;
pub use media::*;
pub use syndicate::*;
pub use posting::*;
pub use interactions::*;
pub use peers::*;
pub use collab::*;
pub use ap::*;
pub use profile::*;
pub use unfurl::*;
pub use browser::*;
