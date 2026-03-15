use async_trait::async_trait;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub type ToolRegistry = BTreeMap<String, Box<dyn Tool>>;

pub struct ToolContext {
    pub agent_dir: PathBuf,
    pub workspace_dir: PathBuf,
    pub sessions_dir: PathBuf,
}

#[async_trait(?Send)]
pub trait Tool {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    async fn execute(&self, args: &Value, ctx: &ToolContext) -> String;

    fn as_tool_payload(&self) -> Value {
        json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": self.parameters(),
            }
        })
    }

    fn as_catalog_entry(&self) -> Value {
        json!({
            "description": self.description(),
            "parameters": self.parameters(),
        })
    }
}
