use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::core::Agent;
use crate::agent::types::{AgentRunResult, ExecutionStep, StepOutcome, ToolCallTrace};
use crate::message::{Message, next_request_id};
use crate::tooling::types::{AskHumanFn, ToolContext};
use std::sync::Arc;

#[async_trait(?Send)]
pub trait AgentLoop {
    async fn run_detailed(
        &self,
        agent: &Agent,
        session_id: &str,
        user_message: &str,
        on_step: Option<std::sync::Arc<dyn Fn(ExecutionStep) + Send + Sync>>,
    ) -> AgentRunResult;

    /// Like `run_detailed`, but also injects an `AskHumanFn` into the
    /// tool context so tools can pause for human input.
    async fn run_detailed_with_human(
        &self,
        agent: &Agent,
        session_id: &str,
        user_message: &str,
        on_step: Option<std::sync::Arc<dyn Fn(ExecutionStep) + Send + Sync>>,
        _human: Option<Arc<dyn AskHumanFn>>,
    ) -> AgentRunResult {
        // Default: ignore human context.
        self.run_detailed(agent, session_id, user_message, on_step)
            .await
    }

    async fn run(&self, agent: &Agent, session_id: &str, user_message: &str) -> String {
        self.run_detailed(agent, session_id, user_message, None)
            .await
            .content
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum LoopPhase {
    Understand,
    Plan,
    Act,
    Observe,
    Recover,
    Finalize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BudgetState {
    pub llm_calls: usize,
    pub tool_calls: usize,
    pub iterations: usize,
    pub retries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionState {
    pub phase: LoopPhase,
    pub budget: BudgetState,
    pub last_error: Option<String>,
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self {
            phase: LoopPhase::Understand,
            budget: BudgetState::default(),
            last_error: None,
        }
    }
}

/// A richer loop result than plain Continue / Final.
/// This is the main extension point for planner/executor and verifier patterns.
#[derive(Debug, Clone)]
pub enum LoopDirective {
    Continue {
        next_phase: LoopPhase,
        tool_calls_made: usize,
        tool_names: Vec<String>,
        tool_traces: Vec<ToolCallTrace>,
    },
    Retry {
        reason: String,
        next_phase: LoopPhase,
    },
    Final(String),
}

pub struct DefaultAgentLoop;

impl DefaultAgentLoop {
    fn summarize_json(value: &Value) -> String {
        let raw = value.to_string();
        Self::truncate_detail(&raw, 160)
    }

    fn truncate_detail(raw: &str, max_len: usize) -> String {
        let mut chars = raw.chars();
        let truncated: String = chars.by_ref().take(max_len).collect();
        if chars.next().is_some() {
            format!("{truncated}...")
        } else {
            truncated
        }
    }

    fn push_step(
        &self,
        on_step_cb: Option<&std::sync::Arc<dyn Fn(ExecutionStep) + Send + Sync>>,
        steps: &mut Vec<ExecutionStep>,
        phase: &LoopPhase,
        kind: impl Into<String>,
        detail: impl Into<String>,
    ) {
        let step = ExecutionStep {
            index: steps.len() + 1,
            phase: format!("{phase:?}"),
            kind: kind.into(),
            detail: detail.into(),
        };
        if let Some(on_step) = on_step_cb {
            on_step(step.clone());
        }
        steps.push(step);
    }

    async fn load_execution_state(&self, agent: &Agent, session_id: &str) -> ExecutionState {
        // Swap this for real persisted state storage when ready.
        // For now, start fresh each run.
        let _ = (agent, session_id);
        ExecutionState::default()
    }

    async fn persist_execution_state(
        &self,
        agent: &Agent,
        session_id: &str,
        state: &ExecutionState,
    ) {
        // Swap this for real persistence alongside message state.
        let _ = (agent, session_id, state);
    }

    fn max_retries(&self) -> usize {
        2
    }

    fn tool_calls_from_messages(&self, messages: &[Message]) -> usize {
        messages
            .iter()
            .filter_map(|message| {
                let value = Value::from(message);
                value
                    .get("tool_calls")
                    .and_then(Value::as_array)
                    .map(Vec::len)
            })
            .sum()
    }

    fn should_stop(&self, agent: &Agent, state: &ExecutionState) -> Option<String> {
        if state.budget.iterations >= agent.definition.max_iterations {
            return Some("Max iterations reached.".to_string());
        }

        // Optional future knobs:
        // - max_llm_calls
        // - max_tool_calls
        // - max_cost
        // Until then, use conservative soft guards.
        let llm_cap = agent.definition.max_iterations;
        if state.budget.llm_calls >= llm_cap {
            return Some("LLM call budget exhausted.".to_string());
        }

        None
    }

    async fn initialize_messages(
        &self,
        agent: &Agent,
        session_id: &str,
        user_message: &str,
    ) -> Result<Vec<Message>, String> {
        let ctx = agent.workspace.tool_context(session_id);

        #[cfg(not(target_arch = "wasm32"))]
        if let Err(e) = tokio::fs::create_dir_all(&ctx.workspace_dir).await {
            return Err(format!("Workspace error: {e}"));
        }

        let mut messages = agent
            .load_messages(session_id)
            .await
            .map_err(|e| format!("Session state error: {e}"))?;

        if messages.is_empty() {
            let memory_context = agent
                .memory
                .build_context(session_id, user_message)
                .await
                .unwrap_or_default();

            messages.push(Message::system(agent.system_prompt(&ctx, &memory_context)));
        }

        let request_id = next_request_id();
        let prev_message_id = messages.last().map(|message| message.message_id.clone());

        messages.push(Message::user(
            user_message.to_string(),
            request_id,
            prev_message_id,
            None,
        ));

        Ok(messages)
    }

    async fn step_with_phase(
        &self,
        agent: &Agent,
        messages: &mut Vec<Message>,
        ctx: &mut ToolContext,
        state: &mut ExecutionState,
    ) -> Result<LoopDirective, String> {
        let tool_calls_before = self.tool_calls_from_messages(messages);

        state.budget.llm_calls += 1;

        // Right now, the underlying agent.step still owns the actual LLM/tool turn.
        // We wrap it with explicit loop semantics.
        match state.phase {
            LoopPhase::Understand
            | LoopPhase::Plan
            | LoopPhase::Act
            | LoopPhase::Observe
            | LoopPhase::Recover => match agent.step(messages, ctx).await {
                Ok(StepOutcome::Continue {
                    tool_names,
                    tool_traces,
                }) => {
                    let tool_calls_after = self.tool_calls_from_messages(messages);
                    let new_tool_calls = tool_calls_after.saturating_sub(tool_calls_before);

                    let next_phase = if new_tool_calls > 0 {
                        LoopPhase::Observe
                    } else {
                        LoopPhase::Act
                    };

                    Ok(LoopDirective::Continue {
                        next_phase,
                        tool_calls_made: new_tool_calls,
                        tool_names,
                        tool_traces,
                    })
                }
                Ok(StepOutcome::Final(content)) => Ok(LoopDirective::Final(content)),
                Err(e) => Ok(LoopDirective::Retry {
                    reason: e.to_string(),
                    next_phase: LoopPhase::Recover,
                }),
            },
            LoopPhase::Finalize => {
                // Defensive fallback.
                Ok(LoopDirective::Final("Done.".to_string()))
            }
        }
    }

    async fn finalize_success(
        &self,
        agent: &Agent,
        session_id: &str,
        user_message: &str,
        messages: &mut Vec<Message>,
        state: &ExecutionState,
        content: String,
        steps: Vec<ExecutionStep>,
    ) -> AgentRunResult {
        let _ = agent
            .memory
            .record_all(session_id, user_message, &content)
            .await;
        let _ = agent.memory.consolidate_all(session_id).await;

        agent.persist_state(session_id, messages).await;
        self.persist_execution_state(agent, session_id, state).await;

        AgentRunResult { content, steps }
    }

    async fn finalize_failure(
        &self,
        agent: &Agent,
        session_id: &str,
        user_message: &str,
        messages: &mut Vec<Message>,
        state: &ExecutionState,
        content: String,
        steps: Vec<ExecutionStep>,
    ) -> AgentRunResult {
        Agent::push_out_message(
            messages,
            serde_json::json!({
                "role": "assistant",
                "content": content.clone(),
            }),
        );

        let _ = agent
            .memory
            .record_all(session_id, user_message, &content)
            .await;
        let _ = agent.memory.consolidate_all(session_id).await;

        agent.persist_state(session_id, messages).await;
        self.persist_execution_state(agent, session_id, state).await;

        AgentRunResult { content, steps }
    }
}

#[async_trait(?Send)]
impl AgentLoop for DefaultAgentLoop {
    async fn run_detailed(
        &self,
        agent: &Agent,
        session_id: &str,
        user_message: &str,
        on_step: Option<std::sync::Arc<dyn Fn(ExecutionStep) + Send + Sync>>,
    ) -> AgentRunResult {
        self.run_detailed_with_human(agent, session_id, user_message, on_step, None)
            .await
    }

    async fn run_detailed_with_human(
        &self,
        agent: &Agent,
        session_id: &str,
        user_message: &str,
        on_step: Option<std::sync::Arc<dyn Fn(ExecutionStep) + Send + Sync>>,
        human: Option<Arc<dyn AskHumanFn>>,
    ) -> AgentRunResult {
        let mut ctx = agent.workspace.tool_context(session_id);
        ctx.human = human;
        let mut steps = Vec::new();

        let mut messages = match self
            .initialize_messages(agent, session_id, user_message)
            .await
        {
            Ok(messages) => messages,
            Err(e) => {
                self.push_step(
                    on_step.as_ref(),
                    &mut steps,
                    &LoopPhase::Understand,
                    "error",
                    format!("Failed to initialize session: {e}"),
                );
                return AgentRunResult { content: e, steps };
            }
        };

        let mut state = self.load_execution_state(agent, session_id).await;
        state.phase = LoopPhase::Understand;
        self.push_step(
            on_step.as_ref(),
            &mut steps,
            &state.phase,
            "start",
            format!("Starting run for session `{session_id}`"),
        );

        tracing::info!(session_id = session_id, "Starting agent execution loop");

        loop {
            let current_phase = state.phase.clone();
            let iteration = state.budget.iterations + 1;
            self.push_step(
                on_step.as_ref(),
                &mut steps,
                &current_phase,
                "iteration",
                format!("Iteration {iteration} entered {current_phase:?}"),
            );
            tracing::info!(
                phase = ?state.phase,
                iteration = state.budget.iterations,
                "Agent loop step"
            );

            if let Some(stop_reason) = self.should_stop(agent, &state) {
                tracing::warn!(reason = %stop_reason, "Stopping agent loop prematurely");
                self.push_step(
                    on_step.as_ref(),
                    &mut steps,
                    &state.phase,
                    "stop",
                    stop_reason.clone(),
                );
                return self
                    .finalize_failure(
                        agent,
                        session_id,
                        user_message,
                        &mut messages,
                        &state,
                        stop_reason,
                        steps,
                    )
                    .await;
            }

            state.budget.iterations += 1;

            let directive = match self
                .step_with_phase(agent, &mut messages, &mut ctx, &mut state)
                .await
            {
                Ok(directive) => directive,
                Err(e) => LoopDirective::Retry {
                    reason: e,
                    next_phase: LoopPhase::Recover,
                },
            };

            match directive {
                LoopDirective::Continue {
                    next_phase,
                    tool_calls_made,
                    tool_names,
                    tool_traces,
                } => {
                    tracing::info!(
                        next_phase = ?next_phase,
                        tool_calls = tool_calls_made,
                        "Continuing agent loop"
                    );
                    for trace in &tool_traces {
                        self.push_step(
                            on_step.as_ref(),
                            &mut steps,
                            &state.phase,
                            "tool_call",
                            format!(
                                "Calling tool `{}` with args {}",
                                trace.name,
                                Self::summarize_json(&trace.args)
                            ),
                        );
                        self.push_step(
                            on_step.as_ref(),
                            &mut steps,
                            &state.phase,
                            "tool_result",
                            format!(
                                "Tool `{}` returned {}",
                                trace.name,
                                Self::truncate_detail(&trace.result, 160)
                            ),
                        );
                    }
                    let detail = if tool_names.is_empty() {
                        format!("No tool call. Advancing to {next_phase:?}")
                    } else {
                        format!(
                            "Executed tool(s): {}. Advancing to {next_phase:?}",
                            tool_names.join(", ")
                        )
                    };
                    self.push_step(
                        on_step.as_ref(),
                        &mut steps,
                        &state.phase,
                        "continue",
                        detail,
                    );

                    state.phase = next_phase;
                    state.budget.tool_calls += tool_calls_made;
                    state.last_error = None;

                    agent.persist_state(session_id, &messages).await;
                    self.persist_execution_state(agent, session_id, &state)
                        .await;
                }

                LoopDirective::Retry { reason, next_phase } => {
                    tracing::warn!(
                        reason = %reason,
                        next_phase = ?next_phase,
                        retries = state.budget.retries,
                        "Retrying agent loop"
                    );
                    self.push_step(
                        on_step.as_ref(),
                        &mut steps,
                        &state.phase,
                        "retry",
                        format!("Retrying after error: {reason}"),
                    );

                    state.budget.retries += 1;
                    state.last_error = Some(reason.clone());
                    state.phase = next_phase;

                    if state.budget.retries > self.max_retries() {
                        let content = format!("LLM error: {reason}");
                        self.push_step(
                            on_step.as_ref(),
                            &mut steps,
                            &state.phase,
                            "failed",
                            content.clone(),
                        );
                        return self
                            .finalize_failure(
                                agent,
                                session_id,
                                user_message,
                                &mut messages,
                                &state,
                                content,
                                steps,
                            )
                            .await;
                    }

                    Agent::push_out_message(
                        &mut messages,
                        serde_json::json!({
                            "role": "assistant",
                            "content": format!("Recovering from error: {reason}"),
                        }),
                    );

                    agent.persist_state(session_id, &messages).await;
                    self.persist_execution_state(agent, session_id, &state)
                        .await;
                }

                LoopDirective::Final(content) => {
                    tracing::info!("Agent loop finalized successfully");
                    state.phase = LoopPhase::Finalize;
                    self.push_step(
                        on_step.as_ref(),
                        &mut steps,
                        &state.phase,
                        "final",
                        "Agent produced a final response".to_string(),
                    );
                    return self
                        .finalize_success(
                            agent,
                            session_id,
                            user_message,
                            &mut messages,
                            &state,
                            content,
                            steps,
                        )
                        .await;
                }
            }
        }
    }
}
