//! Everything that touches the operating system: the parallel directory
//! walk, child processes for cleanups and trash, configuration paths, cache
//! files on disk and the local time zone offset.

pub mod cache_io;
pub mod config;
pub mod dialog;
pub mod localtime;
pub mod meta;
pub mod mft;
pub mod mounts;
pub mod root;
pub mod run;
pub mod trash;
pub mod update;
pub mod walker;

pub use walker::{scan, ScanEvent, ScanHandle, ScanOptions, Wake};
