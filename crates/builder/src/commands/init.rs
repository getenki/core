use crate::cli::{InitArgs, Template};
use std::fs;
use std::path::Path;

// ── Embedded templates ────────────────────────────────────────────────────

const TS_ENKI_TOML: &str = include_str!("../../templates/ts/enki.toml");
const TS_PACKAGE_JSON: &str = include_str!("../../templates/ts/package.json");
const TS_TSCONFIG: &str = include_str!("../../templates/ts/tsconfig.json");
const TS_INDEX: &str = include_str!("../../templates/ts/src/index.ts");

const PY_ENKI_TOML: &str = include_str!("../../templates/py/enki.toml");
const PY_GITIGNORE: &str = include_str!("../../templates/py/.gitignore");
const PY_PYPROJECT: &str = include_str!("../../templates/py/pyproject.toml");
const PY_ASSISTANT_TOOL: &str = include_str!("../../templates/py/src/tools/assistant.py");

const RS_ENKI_TOML: &str = include_str!("../../templates/rs/enki.toml");
const RS_CARGO_TOML: &str = include_str!("../../templates/rs/Cargo.toml.tmpl");
const RS_MAIN: &str = include_str!("../../templates/rs/src/main.rs");

const README_TEMPLATE: &str = r#"# {{PROJECT_NAME}}

An [Enki](https://docs.getenki.com) multi-agent project.

## Getting Started

```bash
# Install dependencies
enki build

# Run agents
enki run --message "Hello, agents!"

# Interactive mode
enki join
```

## Configuration

Edit `enki.toml` to add agents, change models, or update capabilities.
Python projects can define reusable `[[tool]]` entries with `path` and `symbol`.
"#;

const PY_TOOL_BLOCK: &str = r#"
[[tool]]
id = "assistant-tools"
kind = "python"
path = "src/tools/assistant.py"
symbol = "project_runtime_info"
"#;

pub fn run(args: InitArgs) -> Result<(), String> {
    let project_dir = Path::new(&args.name);

    if args.with_tool && !matches!(args.template, Template::Py) {
        return Err("--with-tool is currently supported only with --template py.".to_string());
    }

    if project_dir.exists() {
        return Err(format!(
            "Directory '{}' already exists. Choose a different name.",
            args.name
        ));
    }

    println!(
        "\x1b[1;36m⚡ Creating Enki project\x1b[0m '{}' with {} template...",
        args.name, args.template
    );

    // Create directory structure
    fs::create_dir_all(project_dir.join("src"))
        .map_err(|e| format!("Failed to create directory: {e}"))?;

    // Write enki.toml + README
    let (enki_toml, files) = match args.template {
        Template::Ts => (
            TS_ENKI_TOML.to_string(),
            scaffold_ts(project_dir, &args.name)?,
        ),
        Template::Py => (
            render_py_enki_toml(args.with_tool),
            scaffold_py(project_dir, &args.name, args.with_tool)?,
        ),
        Template::Rs => (
            RS_ENKI_TOML.to_string(),
            scaffold_rs(project_dir, &args.name)?,
        ),
    };

    let enki_toml = enki_toml.replace("{{PROJECT_NAME}}", &args.name);
    write_file(project_dir.join("enki.toml"), &enki_toml)?;

    let readme = README_TEMPLATE.replace("{{PROJECT_NAME}}", &args.name);
    write_file(project_dir.join("README.md"), &readme)?;

    println!();
    println!("\x1b[1;32m✓ Project created!\x1b[0m");
    println!();
    println!("  \x1b[2mFiles:\x1b[0m");
    println!("    {}/enki.toml", args.name);
    println!("    {}/README.md", args.name);
    for file in &files {
        println!("    {}/{}", args.name, file);
    }
    println!();
    println!("  \x1b[2mNext steps:\x1b[0m");
    println!("    cd {}", args.name);
    println!("    enki build");
    println!("    enki run --message \"Hello!\"");
    println!();

    Ok(())
}

fn scaffold_ts(dir: &Path, name: &str) -> Result<Vec<String>, String> {
    let pkg = TS_PACKAGE_JSON.replace("{{PROJECT_NAME}}", name);
    write_file(dir.join("package.json"), &pkg)?;
    write_file(dir.join("tsconfig.json"), TS_TSCONFIG)?;
    write_file(dir.join("src/index.ts"), TS_INDEX)?;
    Ok(vec![
        "package.json".into(),
        "tsconfig.json".into(),
        "src/index.ts".into(),
    ])
}

fn scaffold_py(dir: &Path, name: &str, with_tool: bool) -> Result<Vec<String>, String> {
    write_file(dir.join(".gitignore"), PY_GITIGNORE)?;
    let pyproject = PY_PYPROJECT.replace("{{PROJECT_NAME}}", name);
    write_file(dir.join("pyproject.toml"), &pyproject)?;
    write_file(dir.join("src/__init__.py"), "")?;

    let mut files = vec![
        ".gitignore".into(),
        "pyproject.toml".into(),
        "src/__init__.py".into(),
    ];
    if with_tool {
        write_file(dir.join("src/tools/__init__.py"), "")?;
        write_file(dir.join("src/tools/assistant.py"), PY_ASSISTANT_TOOL)?;
        files.push("src/tools/__init__.py".into());
        files.push("src/tools/assistant.py".into());
    }

    Ok(files)
}

fn scaffold_rs(dir: &Path, name: &str) -> Result<Vec<String>, String> {
    let cargo = RS_CARGO_TOML.replace("{{PROJECT_NAME}}", name);
    write_file(dir.join("Cargo.toml"), &cargo)?;
    write_file(dir.join("src/main.rs"), RS_MAIN)?;
    Ok(vec!["Cargo.toml".into(), "src/main.rs".into()])
}

fn write_file(path: std::path::PathBuf, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Failed to create dir: {e}"))?;
    }
    fs::write(&path, content).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

fn render_py_enki_toml(with_tool: bool) -> String {
    let mut rendered = PY_ENKI_TOML.to_string();
    if with_tool {
        rendered.push_str(PY_TOOL_BLOCK);
        rendered.push('\n');
        rendered.push_str(r#"tools = ["assistant-tools"]"#);
        rendered.push('\n');
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::render_py_enki_toml;

    #[test]
    fn render_py_enki_toml_omits_tool_block_by_default() {
        let rendered = render_py_enki_toml(false);
        assert!(!rendered.contains("[[tool]]"));
        assert!(!rendered.contains("tools = [\"assistant-tools\"]"));
    }

    #[test]
    fn render_py_enki_toml_adds_tool_block_when_requested() {
        let rendered = render_py_enki_toml(true);
        assert!(rendered.contains("[[tool]]"));
        assert!(rendered.contains("path = \"src/tools/assistant.py\""));
        assert!(rendered.contains("tools = [\"assistant-tools\"]"));
    }
}
