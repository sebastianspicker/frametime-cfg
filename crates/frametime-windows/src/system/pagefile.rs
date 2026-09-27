//! Pagefile transaction façade. The implementation is split by the capture,
//! validation, journaling, mutation, and recovery boundaries.

mod binding;
mod journal;
mod live;
mod model;
mod recovery;
#[cfg(test)]
mod tests;
mod validation;

pub(crate) use binding::*;
pub(crate) use journal::*;
#[cfg(any(test, windows))]
pub(crate) use live::*;
pub(crate) use model::*;
#[cfg(any(test, windows))]
pub(crate) use recovery::*;
pub(crate) use validation::*;
