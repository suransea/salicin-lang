//! End-to-end compiler tests.
//!
//! This stays as one integration-test crate so the compiler and CLI are built
//! once. Suites live in separate modules to keep unrelated concerns isolated.

#[path = "suite/support.rs"]
mod support;

#[path = "suite/abi.rs"]
mod abi;
#[path = "suite/async_runtime.rs"]
mod async_runtime;
#[path = "suite/effects.rs"]
mod effects;
#[path = "suite/io.rs"]
mod io;
#[path = "suite/language.rs"]
mod language;
#[path = "suite/ownership.rs"]
mod ownership;
#[path = "suite/packages.rs"]
mod packages;
#[path = "suite/tooling.rs"]
mod tooling;
