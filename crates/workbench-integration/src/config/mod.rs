//! Workbench configuration documents, runtime overrides, setting metadata, and validation.

pub mod catalog;
pub mod chain;
pub mod validate;

mod draft;
mod patch;
mod runtime;
#[cfg(test)]
pub(crate) mod test_support;

pub use draft::{APP_SUPPLIED, ConfigDraft, resolve_secondary_task};
pub use patch::has_comments;
pub use runtime::{Pending, Ready, RunOverrides, WorkbenchConfigHandler};
