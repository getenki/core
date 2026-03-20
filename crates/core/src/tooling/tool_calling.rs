use crate::tooling::types::{Tool, ToolContext, ToolRegistry};
use async_trait::async_trait;
use serde_json::{Value, json};

pub struct ToolCallRegistry {
    tools: ToolRegistry,
}

impl ToolCallRegistry {
    pub fn new(tools: ToolRegistry) -> Self {
        Self { tools }
    }

    pub fn get(&self, tool_name: &str) -> Option<&dyn Tool> {
        self.tools.get(tool_name).map(Box::as_ref)
    }

    pub fn catalog_json(&self) -> Value {
        let mut map = serde_json::Map::new();

        for tool in self.tools.values() {
            map.insert(tool.name().to_string(), tool.as_catalog_entry());
        }

        Value::Object(map)
    }

    pub fn tools_payload(&self) -> Vec<Value> {
        self.tools
            .values()
            .map(|tool| tool.as_tool_payload())
            .collect()
    }
}

fn normalize_tool_args(args: &Value) -> Value {
    match args {
        Value::String(raw) => {
            serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.clone()))
        }
        _ => args.clone(),
    }
}

#[async_trait(?Send)]
pub trait ToolExecutor {
    async fn execute(
        &self,
        registry: &ToolCallRegistry,
        tool_name: &str,
        args: &Value,
        ctx: &ToolContext,
    ) -> String;

    async fn build_tool_message(
        &self,
        registry: &ToolCallRegistry,
        tool_name: &str,
        args: &Value,
        ctx: &ToolContext,
        tool_call_id: Option<&str>,
    ) -> Value {
        let content = self.execute(registry, tool_name, args, ctx).await;
        let mut message = json!({
            "role": "tool",
            "tool_name": tool_name,
            "content": content
        });

        if let Some(tool_call_id) = tool_call_id {
            message["tool_call_id"] = Value::String(tool_call_id.to_string());
        }

        message
    }
}

pub struct RegistryToolExecutor;

#[async_trait(?Send)]
impl ToolExecutor for RegistryToolExecutor {
    async fn execute(
        &self,
        registry: &ToolCallRegistry,
        tool_name: &str,
        args: &Value,
        ctx: &ToolContext,
    ) -> String {
        match registry.get(tool_name) {
            Some(tool) => tool.execute(&normalize_tool_args(args), ctx).await,
            None => format!("Unknown tool: {tool_name}"),
        }
    }
}
