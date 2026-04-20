# Rust Library Examples

This example crate shows how to consume `enki-next` from a separate Rust application crate.

It is intentionally detached from the workspace so it behaves more like an external consumer project that depends on the library through a path dependency:

```toml
[dependencies]
enki_next = { package = "enki-next", path = "../../crates/core" }
```

## What is included

- `runtime_builder_detailed`: single-agent runtime with a custom Rust tool and a mocked LLM provider
- `multi_agent_detailed`: multi-agent orchestration with agent discovery, delegation, and deterministic mocked responses
- `workflow_detailed`: workflow runtime with reusable tasks, inline tasks, a custom transform, a human gate, persisted runs, and resume

## Run the examples

From the repository root:

```powershell
cargo run --manifest-path example/enki-rs/Cargo.toml --bin runtime_builder_detailed
cargo run --manifest-path example/enki-rs/Cargo.toml --bin multi_agent_detailed
cargo run --manifest-path example/enki-rs/Cargo.toml --bin workflow_detailed
```

Each example creates its own temporary workspace and uses mocked providers so it can run without live model credentials.
