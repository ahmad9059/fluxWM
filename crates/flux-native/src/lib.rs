//! Native seat/device lifecycle foundation. Readiness here means a prepared GPU
//! context, not an output, successful modeset, frame or desktop qualification.
mod lifecycle;
pub use lifecycle::*;
#[cfg(feature = "system")]
pub mod system;
