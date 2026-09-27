#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//! dra-workflow backend: hexagonal, feature-sliced. See docs/architecture/backend.md.
//!
//! Layers per feature: `domain` (pure model, rules, port traits) ← `application` (use cases) ←
//! `api` (axum handlers, DTOs); `infra` implements the ports. Only [`bootstrap`] knows adapters.

pub mod app;
pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod features;
pub mod mcp;
pub mod shared;
