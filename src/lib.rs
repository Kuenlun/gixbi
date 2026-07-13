// SPDX-License-Identifier: MIT OR Apache-2.0
// gixbi - Terminal graph of how chosen git branches interact
// Copyright (c) 2026 Juan Luis Leal Contreras (Kuenlun)

#![cfg_attr(coverage_nightly, feature(coverage_attribute))]

//! Core library of `gixbi` (gix branch interaction).
//!
//! Given the tips of two or more branches, the library reconstructs how
//! their histories flowed into each other and renders a compact terminal
//! graph showing only those branches, hiding every intermediate one.

pub mod analysis;
pub mod error;
pub mod render;
pub mod repo;
#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
pub(crate) mod testutil;

pub use error::Error;
