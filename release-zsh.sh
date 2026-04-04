#!/bin/bash
set -euo pipefail

VERSION=${1:-}
TARGETS=${2:-} # Optional: e.g., "js,py" or "rs"

show_help() {
  echo "Usage: ./release-mac.sh [VERSION] [TARGETS]"
  echo "  VERSION: The new version string (e.g., 1.2.0)"
  echo "  TARGETS: Optional comma-separated list of targets: js, py, rs. If omitted, releases all."
  echo ""
  echo "Example All: ./release-mac.sh 1.2.0"
  echo "Example Selective: ./release-mac.sh 1.2.1 js,py"
}

if [[ "$VERSION" == "-h" ]] || [[ "$VERSION" == "--help" ]]; then
  show_help
  exit 0
fi

if [[ -z "$VERSION" ]]; then
  show_help
  exit 1
fi

if [[ -n $(git status --porcelain) ]]; then
  echo "Error: Git directory is dirty. Please commit or stash changes first."
  exit 1
fi

should_update() {
  [[ -z "$TARGETS" ]] || [[ "$TARGETS" == *"$1"* ]]
}

run_cmd() {
  echo ">> $*"
  "$@"
}

replace_first_match() {
  local file=$1
  local pattern=$2
  local replacement=$3
  perl -0pi -e "s/$pattern/$replacement/" "$file"
}

UPDATED_FILES=()
echo "Preparing macOS release for version $VERSION..."

if should_update "rs"; then
  replace_first_match "Cargo.toml" '(\[workspace\.package\][\s\S]*?^version = ").*?(")' "\${1}$VERSION\${2}"
  run_cmd cargo generate-lockfile
  UPDATED_FILES+=("Cargo.toml" "Cargo.lock")
  echo "Updated Cargo.toml"
fi

if should_update "js"; then
  (
    cd ./crates/bindings/enki-js
    run_cmd npm install --no-save
    run_cmd npm version "$VERSION" --no-git-tag-version
  )
  UPDATED_FILES+=("crates/bindings/enki-js/package.json" "crates/bindings/enki-js/package-lock.json")
  echo "Updated crates/bindings/enki-js/package.json"
fi

if should_update "py"; then
  replace_first_match "crates/bindings/enki-py/Cargo.toml" '(\[package\][\s\S]*?^version = ").*?(")' "\${1}$VERSION\${2}"
  run_cmd cargo generate-lockfile
  UPDATED_FILES+=("crates/bindings/enki-py/Cargo.toml" "Cargo.lock")
  echo "Updated crates/bindings/enki-py/Cargo.toml for Python"
fi

echo "Staging release files..."
run_cmd git add "${UPDATED_FILES[@]}"

echo "Creating release commit..."
if [[ -z "$TARGETS" ]]; then
  run_cmd git commit -m "chore: release $VERSION"
else
  run_cmd git commit -m "chore: release $VERSION ($TARGETS)"
fi

if [[ -z "$TARGETS" ]]; then
  echo "Creating global release tag..."
  run_cmd git tag "v$VERSION"
  echo "Created global tag: v$VERSION"
else
  IFS=',' read -r -a selected_targets <<< "$TARGETS"
  for target in "${selected_targets[@]}"; do
    echo "Creating selective release tag for $target..."
    run_cmd git tag "$target-v$VERSION"
    echo "Created selective tag: $target-v$VERSION"
  done
fi

current_branch=$(git branch --show-current)
echo "Pushing commit and tags to origin/$current_branch..."
run_cmd git push origin "$current_branch" --tags
echo "Release $VERSION dispatched to GitHub from branch $current_branch"
