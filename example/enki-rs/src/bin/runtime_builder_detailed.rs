use async_trait::async_trait;
use enki_next::agent::AgentDefinition;
use enki_next::runtime::{RuntimeBuilder, RuntimeRequest};
use enki_next::tooling::types::{Tool, ToolContext, parse_tool_args};
use enki_rs_examples::{MockLlm, mock_text, mock_tool_call, print_steps, temp_workspace, write_file};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
struct SummarizeArgs {
    brief: String,
    audience: String,
}

struct SummarizeBriefTool;

#[async_trait(?Send)]
impl Tool for SummarizeBriefTool {
    fn name(&self) -> &str {
        "summarize_brief"
    }

    fn description(&self) -> &str {
        "Summarize a short product brief for a named audience."
    }

    fn parameters(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "brief": { "type": "string" },
                "audience": { "type": "string" }
            },
            "required": ["brief", "audience"]
        })
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext) -> String {
        let parsed: SummarizeArgs = match parse_tool_args(args) {
            Ok(parsed) => parsed,
            Err(error) => return format!("Error: failed to parse tool arguments: {error}"),
        };

        let word_count = parsed.brief.split_whitespace().count();
        format!(
            "audience={} words={} workspace={} summary=This launch expands workflow runtime coverage and keeps the integration surface library-friendly.",
            parsed.audience,
            word_count,
            ctx.workspace_dir.display()
        )
    }
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let workspace_home = temp_workspace("runtime-builder")?;
    let brief_path = workspace_home.join("inputs").join("release-brief.md");
    write_file(
        &brief_path,
        "# Release Brief\n\nEnki now exposes reusable Rust runtime and workflow APIs that can be embedded in application-owned binaries.",
    )?;

    let runtime = RuntimeBuilder::new(AgentDefinition {
        name: "Embedded Product Assistant".to_string(),
        system_prompt_preamble: "Use the summarize_brief tool exactly once, then produce a launch-ready answer for a product manager.".to_string(),
        model: "mock".to_string(),
        max_iterations: 4,
    })
    .with_workspace_home(workspace_home.clone())
    .with_llm(Box::new(MockLlm::new(vec![
        mock_tool_call(
            "call-1",
            "summarize_brief",
            json!({
                "brief": "Enki now exposes reusable Rust runtime and workflow APIs that can be embedded in application-owned binaries.",
                "audience": "product-manager"
            }),
            "tool_calls",
        ),
        mock_text(
            "Launch note: the embedded Rust runtime now supports app-owned orchestration with reusable workflows and custom tools, which makes library integration straightforward for product teams.",
        ),
    ])))
    .register_tool(SummarizeBriefTool)
    .build()
    .await?;

    let response = runtime
        .process_detailed(
            RuntimeRequest::new(
                "runtime-builder-session",
                "app",
                "Turn the release brief into a launch-ready summary.",
            ),
            None,
        )
        .await?;

    println!("Workspace: {}", workspace_home.display());
    println!("Input brief: {}", brief_path.display());
    print_steps("Execution trace:", &response.steps);
    println!("\nFinal response:\n{}", response.response.content);

    Ok(())
}
