//! Lantern reasoning core.
//!
//! Owns temporal state, evidence, confidence and the context object.
//! Knows nothing about sensors (it consumes measurements) and nothing about UI
//! (it produces a data structure). Both boundaries are deliberate.

pub mod actions;
pub mod agent;
pub mod apps;
pub mod authz;
pub mod calculate;
pub mod capabilities;
pub mod config;
pub mod conversation;
pub mod context;
pub mod dialogue;
pub mod engine;
pub mod environment;
pub mod events;
pub mod evidence;
pub mod facts;
pub mod folders;
pub mod goal;
pub mod intent;
pub mod measurement;
pub mod memory;
pub mod model;
pub mod sensor;
pub mod pipeline;
pub mod plan;
pub mod privacy;
pub mod pump;
pub mod router;
pub mod runtime;
pub mod safety;
pub mod storage;
pub mod store;
pub mod surface;
pub mod task;
pub mod telemetry;
pub mod tools;
pub mod transaction;
pub mod voice;

pub use config::Config;
pub use context::ContextObject;
pub use engine::Engine;
