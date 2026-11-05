//! Everything in kelve that's pure: skills and their parts, the debrief and
//! when one is due, learning from what the person corrected, your agent's
//! character, the hook for each agent, what the person knows already, what
//! agents are told for each job, secret stripping and fencing,
//! versions and checksums for updating, and the shapes the API sends. No
//! IO, and nothing panics on bad input.
//!
//! The shared graph's entries, ideas, relations, lessons and terms
//! (`entry`, `ideas`, `relations`, `lesson`, `matching`, `terms`,
//! `formula`) are no part of kelve any more. They stay here only for
//! kelve's hosted service, which still serves version 1 of its API
//! (`kelve-graph-api`), until it retires it.

pub mod character;
pub mod code;
pub mod debrief;
pub mod due;
pub mod entry;
pub mod fence;
pub mod formula;
pub mod hook;
pub mod hosted;
pub mod ideas;
pub mod instructions;
pub mod learn;
pub mod lesson;
pub mod matching;
pub mod md;
pub mod problems;
pub mod redact;
pub mod relations;
pub mod skills;
pub mod spec;
pub mod terms;
pub mod text;
pub mod time;
pub mod update;
pub mod views;

pub use entry::{Block, Body, Entry};
pub use ideas::IdeaRef;
pub use time::Timestamp;
