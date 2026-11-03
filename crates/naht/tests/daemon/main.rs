//! End-to-end daemon tests driven by a fake Studio HTTP client (the `fake-studio` pattern from
//! architecture §10). Each test starts a real `axum` server over a temp project directory and speaks
//! the `MessagePack` protocol to it, exercising the Stage 4 acceptance criteria.

mod activity;
mod blobs;
mod changes;
mod handshake;
#[cfg(test)]
mod support;
