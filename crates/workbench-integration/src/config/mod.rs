//! Workbench configuration documents, runtime overrides, setting metadata, and validation.

pub mod catalog;
pub mod validate;

mod draft;
mod runtime;

pub use draft::{APP_SUPPLIED, ConfigDraft};
pub use runtime::{Pending, Ready, WorkbenchConfigHandler};
