use crate::cli::{InitArgs, Template};
use std::fs;
use std::path::Path;

// ── Embedded templates ────────────────────────────────────────────────────

const TS_ENKI_TOML: &str = include_str!("../../templates/ts/enki.toml");
const TS_PACKAGE_JSON: &str = include_str!("../../templates/ts/package.json");
const TS_TSCONFIG: &str = include_str!("../../templates/ts/tsconfig.json");
const TS_INDEX: &str = include_str!("../../templates/ts/src/index.ts");

const PY_ENKI_TOML: &str = include_str!("../../templates/py/enki.toml");
const PY_PYPROJECT: &str = include_str!("../../templates/py/pyproject.toml");
const PY_MAIN: &str = include_str!("../../templates/py/src/main.py");

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
"#;

pub fn run(args: InitArgs) -> Result<(), String> {
    let project_dir = Path::new(&args.name);

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
        Template::Ts => (TS_ENKI_TOML, scaffold_ts(project_dir, &args.name)?),
        Template::Py => (PY_ENKI_TOML, scaffold_py(project_dir, &args.name)?),
        Template::Rs => (RS_ENKI_TOML, scaffold_rs(project_dir, &args.name)?),
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

fn scaffold_py(dir: &Path, name: &str) -> Result<Vec<String>, String> {
    let pyproject = PY_PYPROJECT.replace("{{PROJECT_NAME}}", name);
    write_file(dir.join("pyproject.toml"), &pyproject)?;
    write_file(dir.join("src/main.py"), PY_MAIN)?;
    Ok(vec!["pyproject.toml".into(), "src/main.py".into()])
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
