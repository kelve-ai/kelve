//! Everything in kelve that's pure: entries, their blocks and their checks,
//! the markdown written from an entry, ideas as everything points at them,
//! the rules marks keep, what a file read plainly must be, secret
//! stripping, versions and checksums for updating, and the shapes the API
//! sends. No IO, and nothing panics on bad input.

pub mod code;
pub mod entry;
pub mod fence;
pub mod formula;
pub mod hook;
pub mod ideas;
pub mod instructions;
pub mod lens;
pub mod lesson;
pub mod marks;
pub mod matching;
pub mod notes;
pub mod plainly;
pub mod problems;
pub mod redact;
pub mod relations;
pub mod spec;
pub mod terms;
pub mod text;
pub mod time;
pub mod update;
pub mod views;

pub use entry::{Block, Body, Entry};
pub use ideas::IdeaRef;
pub use time::Timestamp;
