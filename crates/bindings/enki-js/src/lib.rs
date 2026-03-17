#![deny(clippy::all)]

use core_next::agent::{Agent as CoreAgent, AgentDefinition};
use napi::bindgen_prelude::AsyncTask;
use napi::{Env, Task};
use napi_derive::napi;
use std::sync::{Arc, Mutex, mpsc};
use std::thread;

const DEFAULT_NAME: &str = "Personal Assistant";
const DEFAULT_SYSTEM_PROMPT: &str = "You are a helpful Personal Assistant agent.";
const DEFAULT_MAX_ITERATIONS: u32 = 20;

struct RunRequest {
  session_id: String,
  user_message: String,
  reply_tx: mpsc::Sender<String>,
}

struct AgentHandle {
  request_tx: Mutex<mpsc::Sender<RunRequest>>,
}

pub struct RunTask {
  inner: Arc<AgentHandle>,
  session_id: String,
  user_message: String,
}

#[napi(js_name = "NativeEnkiAgent")]
pub struct NativeEnkiAgent {
  inner: Arc<AgentHandle>,
}

#[napi]
impl NativeEnkiAgent {
  #[napi(constructor)]
  pub fn new(
    name: Option<String>,
    system_prompt_preamble: Option<String>,
    model: Option<String>,
    max_iterations: Option<u32>,
    workspace_home: Option<String>,
  ) -> napi::Result<Self> {
    let definition = AgentDefinition {
      name: name.unwrap_or_else(|| DEFAULT_NAME.to_string()),
      system_prompt_preamble: system_prompt_preamble
        .unwrap_or_else(|| DEFAULT_SYSTEM_PROMPT.to_string()),
      model: model.unwrap_or_default(),
      max_iterations: max_iterations.unwrap_or(DEFAULT_MAX_ITERATIONS).max(1) as usize,
    };

    let request_tx = spawn_agent_worker(definition, workspace_home)?;

    Ok(Self {
      inner: Arc::new(AgentHandle {
        request_tx: Mutex::new(request_tx),
      }),
    })
  }

  #[napi]
  pub fn run(&self, session_id: String, user_message: String) -> AsyncTask<RunTask> {
    AsyncTask::new(RunTask {
      inner: Arc::clone(&self.inner),
      session_id,
      user_message,
    })
  }
}

impl Task for RunTask {
  type Output = String;
  type JsValue = String;

  fn compute(&mut self) -> napi::Result<Self::Output> {
    let (reply_tx, reply_rx) = mpsc::channel();
    let request = RunRequest {
      session_id: self.session_id.clone(),
      user_message: self.user_message.clone(),
      reply_tx,
    };

    let sender =
      self.inner.request_tx.lock().map_err(|_| {
        napi::Error::from_reason("Worker error: request mutex poisoned".to_string())
      })?;

    sender.send(request).map_err(|_| {
      napi::Error::from_reason("Worker error: agent worker has stopped".to_string())
    })?;

    reply_rx
      .recv()
      .map_err(|_| napi::Error::from_reason("Worker error: reply channel dropped".to_string()))
  }

  fn resolve(&mut self, _env: Env, output: Self::Output) -> napi::Result<Self::JsValue> {
    Ok(output)
  }
}

fn spawn_agent_worker(
  definition: AgentDefinition,
  workspace_home: Option<String>,
) -> napi::Result<mpsc::Sender<RunRequest>> {
  let workspace_home = workspace_home.map(Into::into);
  let (request_tx, request_rx) = mpsc::channel::<RunRequest>();
  let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

  thread::spawn(move || {
    let runtime = match tokio::runtime::Builder::new_current_thread()
      .enable_all()
      .build()
    {
      Ok(runtime) => runtime,
      Err(error) => {
        let _ = ready_tx.send(Err(format!(
          "Initialization error: failed to create tokio runtime: {error}"
        )));
        for request in request_rx {
          let _ = request
            .reply_tx
            .send("Initialization error: failed to create tokio runtime".to_string());
        }
        return;
      }
    };

    let agent = match runtime.block_on(CoreAgent::with_definition_executor_and_workspace(
      definition,
      Box::new(core_next::tooling::tool_calling::RegistryToolExecutor),
      workspace_home,
    )) {
      Ok(agent) => agent,
      Err(error) => {
        let message = format!("Initialization error: {error}");
        let _ = ready_tx.send(Err(message.clone()));
        for request in request_rx {
          let _ = request.reply_tx.send(message.clone());
        }
        return;
      }
    };

    let _ = ready_tx.send(Ok(()));

    for request in request_rx {
      let response = runtime.block_on(agent.run(&request.session_id, &request.user_message));
      let _ = request.reply_tx.send(response);
    }
  });

  ready_rx
    .recv()
    .map_err(|_| napi::Error::from_reason("Initialization error: agent worker exited".to_string()))?
    .map_err(napi::Error::from_reason)?;

  Ok(request_tx)
}
