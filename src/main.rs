use tokio::time::{Duration, sleep};

struct AgentState {
    counter: u32,
}

async fn observe(state: &AgentState) -> String {
    format!("Current counter is {}", state.counter)
}

async fn think(observation: &str) -> String {
    println!("Agent thinking about: {}", observation);
    "increment".to_string()
}

async fn act(state: &mut AgentState, action: &str) {
    match action {
        "increment" => {
            state.counter += 1;
            println!("Action: increment -> {}", state.counter);
        }
        _ => println!("Unknown action"),
    }
}

#[tokio::main]
async fn main() {
    let mut state = AgentState { counter: 0 };

    loop {
        // 1. Observe
        let observation = observe(&state).await;

        // 2. Decide
        let action = think(&observation).await;

        // 3. Act
        act(&mut state, &action).await;

        // 4. Wait before next loop
        sleep(Duration::from_secs(1)).await;
    }
}
