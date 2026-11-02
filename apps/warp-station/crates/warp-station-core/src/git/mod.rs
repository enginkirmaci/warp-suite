//! Git: in-process reads via git2 and CLI mutations through the shared
//! process runner, where hooks, gpg signing and credential helpers keep
//! applying.

pub mod cli;
pub mod read;
