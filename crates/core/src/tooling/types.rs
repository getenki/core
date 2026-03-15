use async_trait::async_trait;
use futures::future::LocalBoxFuture;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;

pub type ToolRegistry = BTreeMap<String, Box<dyn Tool>>;

#[derive(Clone)]
pub struct ToolContext {
    pub agent_dir: PathBuf,
    pub workspace_dir: PathBuf,
    pub sessions_dir: PathBuf,
}

pub trait FromToolValue: Sized {
    fn from_tool_value(value: Value) -> Result<Self, String>;
}

impl<T> FromToolValue for T
where
    T: DeserializeOwned,
{
    fn from_tool_value(value: Value) -> Result<Self, String> {
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

pub trait ToolParams: DeserializeOwned {
    fn schema() -> Value;
}

pub trait IntoToolOutput {
    fn into_tool_output(self) -> String;
}

impl IntoToolOutput for String {
    fn into_tool_output(self) -> String {
        self
    }
}

impl IntoToolOutput for &str {
    fn into_tool_output(self) -> String {
        self.to_string()
    }
}

impl<T, E> IntoToolOutput for Result<T, E>
where
    T: IntoToolOutput,
    E: std::fmt::Display,
{
    fn into_tool_output(self) -> String {
        match self {
            Ok(value) => value.into_tool_output(),
            Err(error) => format!("Error: {error}"),
        }
    }
}

type BoxedFunctionToolHandler = Box<dyn Fn(&ToolContext, Vec<Value>) -> String>;
type BoxedAsyncFunctionToolHandler = Box<dyn Fn(ToolContext, Vec<Value>) -> LocalBoxFuture<'static, String>>;
type BoxedTypedFunctionToolHandler = Box<dyn Fn(&ToolContext, Value) -> String>;
type BoxedTypedAsyncFunctionToolHandler = Box<dyn Fn(ToolContext, Value) -> LocalBoxFuture<'static, String>>;

pub trait IntoFunctionToolHandler<Args> {
    fn into_handler(self) -> BoxedFunctionToolHandler;
}

pub trait IntoAsyncFunctionToolHandler<Args> {
    fn into_async_handler(self) -> BoxedAsyncFunctionToolHandler;
}

pub trait IntoTypedFunctionToolHandler<Params> {
    fn into_typed_handler(self) -> BoxedTypedFunctionToolHandler;
}

pub trait IntoTypedAsyncFunctionToolHandler<Params> {
    fn into_typed_async_handler(self) -> BoxedTypedAsyncFunctionToolHandler;
}

impl<F, R> IntoFunctionToolHandler<()> for F
where
    F: Fn(&ToolContext) -> R + 'static,
    R: IntoToolOutput,
{
    fn into_handler(self) -> BoxedFunctionToolHandler {
        Box::new(move |ctx, values| {
            if !values.is_empty() {
                return format!(
                    "Error: expected 0 tool arguments, received {}",
                    values.len()
                );
            }

            (self)(ctx).into_tool_output()
        })
    }
}

impl<F, Fut, R> IntoAsyncFunctionToolHandler<()> for F
where
    F: Fn(ToolContext) -> Fut + 'static,
    Fut: Future<Output = R> + 'static,
    R: IntoToolOutput,
{
    fn into_async_handler(self) -> BoxedAsyncFunctionToolHandler {
        let handler = Arc::new(self);
        Box::new(move |ctx, values| {
            let handler = Arc::clone(&handler);
            Box::pin(async move {
                if !values.is_empty() {
                    return format!(
                        "Error: expected 0 tool arguments, received {}",
                        values.len()
                    );
                }

                (handler)(ctx).await.into_tool_output()
            })
        })
    }
}

impl<F, P, R> IntoTypedFunctionToolHandler<P> for F
where
    F: Fn(&ToolContext, P) -> R + 'static,
    P: ToolParams + 'static,
    R: IntoToolOutput,
{
    fn into_typed_handler(self) -> BoxedTypedFunctionToolHandler {
        Box::new(move |ctx, args| match P::from_tool_value(args) {
            Ok(params) => (self)(ctx, params).into_tool_output(),
            Err(error) => format!("Error: failed to parse tool arguments: {error}"),
        })
    }
}

impl<F, Fut, P, R> IntoTypedAsyncFunctionToolHandler<P> for F
where
    F: Fn(ToolContext, P) -> Fut + 'static,
    Fut: Future<Output = R> + 'static,
    P: ToolParams + 'static,
    R: IntoToolOutput,
{
    fn into_typed_async_handler(self) -> BoxedTypedAsyncFunctionToolHandler {
        let handler = Arc::new(self);
        Box::new(move |ctx, args| {
            let handler = Arc::clone(&handler);
            Box::pin(async move {
                match P::from_tool_value(args) {
                    Ok(params) => (handler)(ctx, params).await.into_tool_output(),
                    Err(error) => format!("Error: failed to parse tool arguments: {error}"),
                }
            })
        })
    }
}

macro_rules! impl_into_function_tool_handler {
    ($($arg:ident => $idx:tt),* $(,)?) => {
        #[allow(non_snake_case)]
        impl<F, R, $($arg),*> IntoFunctionToolHandler<($($arg,)*)> for F
        where
            F: Fn(&ToolContext, $($arg),*) -> R + 'static,
            R: IntoToolOutput,
            $($arg: FromToolValue + 'static),*
        {
            fn into_handler(self) -> BoxedFunctionToolHandler {
                Box::new(move |ctx, values| {
                    let expected = [$($idx),*].len();
                    if values.len() != expected {
                        return format!(
                            "Error: expected {expected} tool arguments, received {}",
                            values.len()
                        );
                    }

                    $(
                        let $arg = match <$arg as FromToolValue>::from_tool_value(values[$idx].clone()) {
                            Ok(value) => value,
                            Err(error) => {
                                return format!(
                                    "Error: failed to parse tool argument at index {}: {error}",
                                    $idx
                                );
                            }
                        };
                    )*

                    (self)(ctx, $($arg),*).into_tool_output()
                })
            }
        }
    };
}

macro_rules! impl_into_async_function_tool_handler {
    ($($arg:ident => $idx:tt),* $(,)?) => {
        #[allow(non_snake_case)]
        impl<F, Fut, R, $($arg),*> IntoAsyncFunctionToolHandler<($($arg,)*)> for F
        where
            F: Fn(ToolContext, $($arg),*) -> Fut + 'static,
            Fut: Future<Output = R> + 'static,
            R: IntoToolOutput,
            $($arg: FromToolValue + 'static),*
        {
            fn into_async_handler(self) -> BoxedAsyncFunctionToolHandler {
                let handler = Arc::new(self);
                Box::new(move |ctx, values| {
                    let handler = Arc::clone(&handler);
                    Box::pin(async move {
                        let expected = [$($idx),*].len();
                        if values.len() != expected {
                            return format!(
                                "Error: expected {expected} tool arguments, received {}",
                                values.len()
                            );
                        }

                        $(
                            let $arg = match <$arg as FromToolValue>::from_tool_value(values[$idx].clone()) {
                                Ok(value) => value,
                                Err(error) => {
                                    return format!(
                                        "Error: failed to parse tool argument at index {}: {error}",
                                        $idx
                                    );
                                }
                            };
                        )*

                        (handler)(ctx, $($arg),*).await.into_tool_output()
                    })
                })
            }
        }
    };
}

impl_into_function_tool_handler!(A0 => 0);
impl_into_function_tool_handler!(A0 => 0, A1 => 1);
impl_into_function_tool_handler!(A0 => 0, A1 => 1, A2 => 2);
impl_into_function_tool_handler!(A0 => 0, A1 => 1, A2 => 2, A3 => 3);
impl_into_function_tool_handler!(A0 => 0, A1 => 1, A2 => 2, A3 => 3, A4 => 4);
impl_into_function_tool_handler!(A0 => 0, A1 => 1, A2 => 2, A3 => 3, A4 => 4, A5 => 5);
impl_into_async_function_tool_handler!(A0 => 0);
impl_into_async_function_tool_handler!(A0 => 0, A1 => 1);
impl_into_async_function_tool_handler!(A0 => 0, A1 => 1, A2 => 2);
impl_into_async_function_tool_handler!(A0 => 0, A1 => 1, A2 => 2, A3 => 3);
impl_into_async_function_tool_handler!(A0 => 0, A1 => 1, A2 => 2, A3 => 3, A4 => 4);
impl_into_async_function_tool_handler!(A0 => 0, A1 => 1, A2 => 2, A3 => 3, A4 => 4, A5 => 5);

enum FunctionToolHandler {
    Sync(BoxedFunctionToolHandler),
    Async(BoxedAsyncFunctionToolHandler),
    TypedSync(BoxedTypedFunctionToolHandler),
    TypedAsync(BoxedTypedAsyncFunctionToolHandler),
}

pub struct FunctionTool {
    name: String,
    description: String,
    parameters: Value,
    param_names: Option<Vec<String>>,
    handler: FunctionToolHandler,
}

impl FunctionTool {
    pub fn from_fn<Args, F>(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
        param_names: impl IntoIterator<Item = impl Into<String>>,
        handler: F,
    ) -> Self
    where
        F: IntoFunctionToolHandler<Args>,
    {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
            param_names: Some(param_names.into_iter().map(Into::into).collect()),
            handler: FunctionToolHandler::Sync(handler.into_handler()),
        }
    }

    pub fn from_async_fn<Args, F>(
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
        param_names: impl IntoIterator<Item = impl Into<String>>,
        handler: F,
    ) -> Self
    where
        F: IntoAsyncFunctionToolHandler<Args>,
    {
        Self {
            name: name.into(),
            description: description.into(),
            parameters,
            param_names: Some(param_names.into_iter().map(Into::into).collect()),
            handler: FunctionToolHandler::Async(handler.into_async_handler()),
        }
    }

    pub fn from_typed_fn<P, F>(
        name: impl Into<String>,
        description: impl Into<String>,
        handler: F,
    ) -> Self
    where
        P: ToolParams + 'static,
        F: IntoTypedFunctionToolHandler<P>,
    {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: P::schema(),
            param_names: None,
            handler: FunctionToolHandler::TypedSync(handler.into_typed_handler()),
        }
    }

    pub fn from_typed_async_fn<P, F>(
        name: impl Into<String>,
        description: impl Into<String>,
        handler: F,
    ) -> Self
    where
        P: ToolParams + 'static,
        F: IntoTypedAsyncFunctionToolHandler<P>,
    {
        Self {
            name: name.into(),
            description: description.into(),
            parameters: P::schema(),
            param_names: None,
            handler: FunctionToolHandler::TypedAsync(handler.into_typed_async_handler()),
        }
    }

    fn ordered_values(&self, args: &Value) -> Result<Vec<Value>, String> {
        let Some(param_names) = &self.param_names else {
            return Err("tool arguments are not positional for this tool".to_string());
        };

        match args {
            Value::Array(values) => Ok(values.clone()),
            Value::Object(map) => param_names
                .iter()
                .map(|name| {
                    map.get(name)
                        .cloned()
                        .ok_or_else(|| format!("missing required tool argument '{name}'"))
                })
                .collect(),
            Value::Null => Ok(Vec::new()),
            _ if param_names.len() == 1 => Ok(vec![args.clone()]),
            _ => Err("tool arguments must be a JSON object or array".to_string()),
        }
    }
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

#[async_trait(?Send)]
impl Tool for FunctionTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters(&self) -> Value {
        self.parameters.clone()
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> String {
        match &self.handler {
            FunctionToolHandler::Sync(handler) => match self.ordered_values(args) {
                Ok(values) => handler(ctx, values),
                Err(error) => format!("Error: {error}"),
            },
            FunctionToolHandler::Async(handler) => match self.ordered_values(args) {
                Ok(values) => handler(ctx.clone(), values).await,
                Err(error) => format!("Error: {error}"),
            },
            FunctionToolHandler::TypedSync(handler) => handler(ctx, args.clone()),
            FunctionToolHandler::TypedAsync(handler) => handler(ctx.clone(), args.clone()).await,
        }
    }
}

#[derive(Default)]
pub struct ToolRegistryBuilder {
    tools: ToolRegistry,
}

impl ToolRegistryBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register<T>(mut self, tool: T) -> Self
    where
        T: Tool + 'static,
    {
        self.tools.insert(tool.name().to_string(), Box::new(tool));
        self
    }

    pub fn register_boxed(mut self, tool: Box<dyn Tool>) -> Self {
        self.tools.insert(tool.name().to_string(), tool);
        self
    }

    pub fn register_fn<Args, F>(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
        param_names: impl IntoIterator<Item = impl Into<String>>,
        handler: F,
    ) -> Self
    where
        F: IntoFunctionToolHandler<Args> + 'static,
    {
        let tool = FunctionTool::from_fn(name, description, parameters, param_names, handler);
        self.tools.insert(tool.name().to_string(), Box::new(tool));
        self
    }

    pub fn register_async_fn<Args, F>(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        parameters: Value,
        param_names: impl IntoIterator<Item = impl Into<String>>,
        handler: F,
    ) -> Self
    where
        F: IntoAsyncFunctionToolHandler<Args> + 'static,
    {
        let tool = FunctionTool::from_async_fn(name, description, parameters, param_names, handler);
        self.tools.insert(tool.name().to_string(), Box::new(tool));
        self
    }

    pub fn register_typed_fn<P, F>(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        handler: F,
    ) -> Self
    where
        P: ToolParams + 'static,
        F: IntoTypedFunctionToolHandler<P> + 'static,
    {
        let tool = FunctionTool::from_typed_fn::<P, F>(name, description, handler);
        self.tools.insert(tool.name().to_string(), Box::new(tool));
        self
    }

    pub fn register_typed_async_fn<P, F>(
        mut self,
        name: impl Into<String>,
        description: impl Into<String>,
        handler: F,
    ) -> Self
    where
        P: ToolParams + 'static,
        F: IntoTypedAsyncFunctionToolHandler<P> + 'static,
    {
        let tool = FunctionTool::from_typed_async_fn::<P, F>(name, description, handler);
        self.tools.insert(tool.name().to_string(), Box::new(tool));
        self
    }

    pub fn build(self) -> ToolRegistry {
        self.tools
    }
}

#[cfg(test)]
mod tests {
    use super::{FunctionTool, Tool, ToolContext, ToolParams, ToolRegistryBuilder};
    use serde::Deserialize;
    use serde_json::{Value, json};
    use std::path::PathBuf;

    fn tool_context() -> ToolContext {
        ToolContext {
            agent_dir: PathBuf::from("agent"),
            workspace_dir: PathBuf::from("workspace"),
            sessions_dir: PathBuf::from("sessions"),
        }
    }

    #[tokio::test]
    async fn function_tool_maps_named_object_args_to_function_params() {
        fn join_paths(ctx: &ToolContext, path: String, suffix: String) -> String {
            format!("{}/{}{}", ctx.workspace_dir.display(), path, suffix)
        }

        let tool = FunctionTool::from_fn(
            "join_paths",
            "Join values",
            json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "suffix": { "type": "string" }
                },
                "required": ["path", "suffix"]
            }),
            ["path", "suffix"],
            join_paths,
        );

        let result = tool
            .execute(
                &json!({
                    "suffix": ".md",
                    "path": "note"
                }),
                &tool_context(),
            )
            .await;

        assert!(result.ends_with("workspace/note.md"));
    }

    #[tokio::test]
    async fn registry_builder_registers_function_tools() {
        fn echo(_ctx: &ToolContext, value: String) -> String {
            format!("echo:{value}")
        }

        let registry = ToolRegistryBuilder::new()
            .register_fn(
                "echo",
                "Echo a value",
                json!({
                    "type": "object",
                    "properties": {
                        "value": { "type": "string" }
                    },
                    "required": ["value"]
                }),
                ["value"],
                echo,
            )
            .build();

        let result = registry
            .get("echo")
            .unwrap()
            .execute(&json!({ "value": "hello" }), &tool_context())
            .await;

        assert_eq!(result, "echo:hello");
    }

    #[tokio::test]
    async fn registry_builder_registers_async_function_tools() {
        async fn echo_async(_ctx: ToolContext, value: String) -> String {
            format!("async:{value}")
        }

        let registry = ToolRegistryBuilder::new()
            .register_async_fn(
                "echo_async",
                "Echo a value asynchronously",
                json!({
                    "type": "object",
                    "properties": {
                        "value": { "type": "string" }
                    },
                    "required": ["value"]
                }),
                ["value"],
                echo_async,
            )
            .build();

        let result = registry
            .get("echo_async")
            .unwrap()
            .execute(&json!({ "value": "hello" }), &tool_context())
            .await;

        assert_eq!(result, "async:hello");
    }

    #[derive(Deserialize)]
    struct EchoParams {
        value: String,
    }

    impl ToolParams for EchoParams {
        fn schema() -> Value {
            json!({
                "type": "object",
                "properties": {
                    "value": { "type": "string" }
                },
                "required": ["value"]
            })
        }
    }

    #[tokio::test]
    async fn registry_builder_registers_typed_function_tools() {
        fn echo(_ctx: &ToolContext, params: EchoParams) -> String {
            format!("typed:{}", params.value)
        }

        let registry = ToolRegistryBuilder::new()
            .register_typed_fn::<EchoParams, _>("echo_typed", "Echo a typed value", echo)
            .build();

        let result = registry
            .get("echo_typed")
            .unwrap()
            .execute(&json!({ "value": "hello" }), &tool_context())
            .await;

        assert_eq!(result, "typed:hello");
    }

    #[tokio::test]
    async fn registry_builder_registers_typed_async_function_tools() {
        async fn echo_async(_ctx: ToolContext, params: EchoParams) -> String {
            format!("typed-async:{}", params.value)
        }

        let registry = ToolRegistryBuilder::new()
            .register_typed_async_fn::<EchoParams, _>(
                "echo_typed_async",
                "Echo a typed value asynchronously",
                echo_async,
            )
            .build();

        let result = registry
            .get("echo_typed_async")
            .unwrap()
            .execute(&json!({ "value": "hello" }), &tool_context())
            .await;

        assert_eq!(result, "typed-async:hello");
    }
}
