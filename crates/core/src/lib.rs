#[macro_use]
mod macros;

pub mod agent;
pub mod llm;
pub mod memory;
pub mod message;
pub mod runtime;
pub mod tooling;

use crate::tooling::builtin_tools::{ExecTool, ReadFileTool, WriteFileTool};
use crate::tooling::types::ToolRegistry;

pub fn default_tool_registry() -> ToolRegistry {
    register_tools![ReadFileTool, WriteFileTool, ExecTool]
}
