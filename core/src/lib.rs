pub use reflex::{ReflexId, ReflexMappingTracker};
pub use stage::{Config, FrameAggregator, FrameId, StageId};
pub use task::{FrameStageStats, TaskAccumulator, TaskStats};

#[cfg(all(feature = "dx12", target_os = "windows"))]
pub mod dx12;
mod entrypoint;
mod ewma;
mod fence_worker;

#[cfg(feature = "profiler")]
mod profiler;
mod reflex;
mod stage;
mod task;
pub mod time;
#[cfg(feature = "vulkan")]
pub mod vulkan;

pub type Timestamp = u64;
pub type Interval = u64;
