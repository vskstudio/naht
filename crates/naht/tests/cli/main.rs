//! Acceptance tests for the Stage 5 commands, driving `naht::commands` directly over temp project
//! directories.

mod build;
mod conflicts;
mod init;
mod release;
mod rojo;
mod serve;
#[cfg(test)]
mod support;
