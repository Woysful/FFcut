//! FFcut core — everything that is not UI.
//!
//! Data flow (the whole app in one picture):
//!
//! ```text
//!  probe(file) ──► SourceInfo ─┐
//!  UI widgets ──► ExportSettings ─┴─► builder::build_args() ──► Vec<String>
//!                                                                  │
//!               cmdline::to_text()  ◄──────────────────────────────┤
//!                     │  (user edits text in the "final command" box)
//!                     ▼
//!               cmdline::parse() ──► Vec<String> ──► run::Job (executes EXACTLY this argv)
//! ```
//!
//! The argv is the single source of truth at export time. Nothing is
//! re-interpreted or filtered after the user edits it.

pub mod audiofx;
pub mod builder;
pub mod cmdline;
pub mod model;
pub mod presets;
pub mod preview;
pub mod probe;
pub mod run;
pub mod tools;
pub mod waveform;

pub use builder::build_args;
pub use model::*;
