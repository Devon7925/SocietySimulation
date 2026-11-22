//! Society: an agent-based town in which every desire is derived from a small
//! set of base preferences, and the law is data the townspeople respond to.
//! See docs/DESIGN.md for the architecture and the contracts between modules.
#![allow(clippy::too_many_arguments, clippy::needless_range_loop, clippy::type_complexity)]

pub mod expr;
pub mod rng;
pub mod types;
pub mod statute;
pub mod polity;
pub mod scope;
pub mod decide;

pub mod worldgen;
pub mod economy;
pub mod law;
pub mod enforce;
pub mod gov;
pub mod life;
pub mod demography;
pub mod research;

pub mod forecast;
pub mod stats;
pub mod engine;
pub mod api;
pub mod testkit;
