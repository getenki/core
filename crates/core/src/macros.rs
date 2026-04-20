#[allow(unused_macros)]
macro_rules! define_tool {
    (
        $tool_type:ty,
        name: $name:literal,
        description: $description:literal,
        parameters: $parameters:expr,
        |$args:ident, $ctx:ident| $body:block
    ) => {
        #[async_trait::async_trait(?Send)]
        impl Tool for $tool_type {
            fn name(&self) -> &str {
                $name
            }

            fn description(&self) -> &str {
                $description
            }

            fn parameters(&self) -> Value {
                $parameters
            }

            async fn execute(&self, $args: &serde_json::Value, $ctx: &crate::tooling::types::ToolContext) -> String $body
        }
    };
}

#[allow(unused_macros)]
macro_rules! register_tools {
    ($($tool_type:ident),+ $(,)?) => {{
        let mut registry: crate::tooling::types::ToolRegistry = std::collections::BTreeMap::new();
        $(
            let tool: std::sync::Arc<dyn crate::tooling::types::Tool> = std::sync::Arc::new($tool_type);
            registry.insert(tool.name().to_string(), tool);
        )+
        registry
    }};
}
