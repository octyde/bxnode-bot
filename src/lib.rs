//! BXNode Bot - AI agent gateway with multi-platform messaging support
//!
//! This crate provides a single binary deployment for an AI agent gateway
//! that supports multiple messaging platforms and LLM providers.

pub mod agent;
pub mod apps;
pub mod channels;
pub mod cli;
pub mod config;
pub mod cron;
pub mod gateway;
pub mod memory;
pub mod plugins;
pub mod project;
pub mod providers;
pub mod session;
pub mod skillgen;
pub mod skills;

pub use config::Config;
