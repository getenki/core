use enki_next::agent::AgentDefinition;
use enki_next::runtime::MultiAgentRuntime;
use enki_rs_examples::{MockLlm, mock_text, mock_tool_call, print_steps, temp_workspace};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), String> {
    let workspace_home = temp_workspace("multi-agent")?;

    let coordinator_llm = MockLlm::new(vec![
        mock_tool_call(
            "call-1",
            "discover_agents",
            json!({ "capability": "research" }),
            "tool_calls",
        ),
        mock_tool_call(
            "call-2",
            "delegate_task",
            json!({
                "agent_id": "researcher",
                "task": "Review the release plan and report the most important integration risk."
            }),
            "tool_calls",
        ),
        mock_text(
            "I delegated the release-plan review to the researcher. The main integration risk is drift between the library-facing API and the examples shipped to consumers.",
        ),
    ]);

    let researcher_llm = MockLlm::new(vec![mock_text(
        "Top integration risk: consumer examples can fall behind the exported Rust API, so published library examples should be validated whenever constructors or workflow types change.",
    )]);

    let runtime = MultiAgentRuntime::builder()
        .add_agent_with_llm(
            "coordinator",
            AgentDefinition {
                name: "Coordinator".to_string(),
                system_prompt_preamble: "Discover the best peer, delegate the specialist work, then return a concise summary.".to_string(),
                model: "mock".to_string(),
                max_iterations: 6,
            },
            vec!["planning".to_string(), "orchestration".to_string()],
            Box::new(coordinator_llm),
        )
        .add_agent_with_llm(
            "researcher",
            AgentDefinition {
                name: "Researcher".to_string(),
                system_prompt_preamble: "Answer delegated release-planning questions with crisp, practical findings.".to_string(),
                model: "mock".to_string(),
                max_iterations: 4,
            },
            vec!["research".to_string(), "analysis".to_string()],
            Box::new(researcher_llm),
        )
        .with_workspace_home(workspace_home.clone())
        .build()
        .await?;

    println!("Workspace: {}", workspace_home.display());
    println!("Registered agents:");
    for card in runtime.registry().list_all().await {
        println!(
            "  - id={} name={} capabilities={:?}",
            card.agent_id, card.name, card.capabilities
        );
    }

    let result = runtime
        .process_detailed(
            "coordinator",
            "multi-agent-session",
            "Review the release plan and call in the right specialist if needed.",
            None,
        )
        .await?;

    print_steps("\nCoordinator execution trace:", &result.steps);
    println!("\nFinal response:\n{}", result.content);

    Ok(())
}
