mod manager;
mod router;
#[cfg(not(target_arch = "wasm32"))]
mod sliding_window;
#[cfg(not(target_arch = "wasm32"))]
mod structured;
#[cfg(not(target_arch = "wasm32"))]
mod summary;
mod types;

pub use manager::MemoryManager;
pub use router::DefaultMemoryRouter;
#[cfg(not(target_arch = "wasm32"))]
pub use sliding_window::SlidingWindowMemory;
#[cfg(not(target_arch = "wasm32"))]
pub use structured::StructuredMemory;
#[cfg(not(target_arch = "wasm32"))]
pub use summary::SummaryMemory;
pub use types::{MemoryEntry, MemoryKind, MemoryProvider, MemoryRouter, MemoryStrategy};

#[cfg(test)]
mod tests;
