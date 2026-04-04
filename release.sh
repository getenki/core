#!/bin/bash
set -euo pipefail

VERSION=${1:-}
TARGETS=${2:-} # Optional: e.g., "js,py" or "rs"

# Usage help
if [[ "$VERSION" == "-h" ]] || [[ "$VERSION" == "--help" ]]; then
  echo "Usage: ./release.sh [VERSION] [TARGETS]"
  echo "  VERSION: The new version string (e.g., 1.2.0)"
  echo "  TARGETS: Optional comma-separated list of targets: js, py, rs. If omitted, releases all."
  echo ""
  echo "Example All: ./release.sh 1.2.0"
  echo "Example Selective: ./release.sh 1.2.1 js,py"
  exit 0
fi

if [ -z "$VERSION" ]; then
  echo "Usage: ./release.sh [VERSION] [TARGETS]"
  echo "Run ./release.sh --help for more information."
  exit 1
fi

# 1. Cleanliness Check
if [[ -n $(git status --porcelain) ]]; then
  echo "❌ Error: Git directory is dirty. Please commit or stash changes first."
  exit 1
fi

# Function to check targets
should_update() { [[ -z "$TARGETS" ]] || [[ "$TARGETS" == *"$1"* ]]; }
run_cmd() {
  echo ">> $*"
  "$@"
}

UPDATED_FILES=""
echo "🚀 Preparing release for version $VERSION..."

# 2. Update Manifests (Adjust paths if files are in subfolders)
if should_update "rs"; then
  perl -0pi -e "s/(\[workspace\.package\][\s\S]*?^version = \").*?(\")/\${1}$VERSION\${2}/m" Cargo.toml
  run_cmd cargo generate-lockfile
  UPDATED_FILES+=" Cargo.toml Cargo.lock"
  echo "✅ Updated Cargo.toml"
fi

if should_update "js"; then
  (
    cd ./crates/bindings/enki-js
    run_cmd npm install --no-save
    run_cmd npm version "$VERSION" --no-git-tag-version
  )
  UPDATED_FILES+=" crates/bindings/enki-js/package.json crates/bindings/enki-js/package-lock.json"
  echo "✅ Updated package.json"
fi

if should_update "py"; then
  perl -0pi -e "s/(\[package\][\s\S]*?^version = \").*?(\")/\${1}$VERSION\${2}/m" crates/bindings/enki-py/Cargo.toml
  run_cmd cargo generate-lockfile
  UPDATED_FILES+=" crates/bindings/enki-py/Cargo.toml Cargo.lock"
  echo "✅ Updated crates/bindings/enki-py/Cargo.toml for Python"
fi

# 3. Commit and Tag
echo "Staging release files..."
run_cmd git add $UPDATED_FILES
if [ -z "$TARGETS" ]; then
  echo "Creating release commit..."
  run_cmd git commit -m "chore: release $VERSION"
else
  echo "Creating release commit..."
  run_cmd git commit -m "chore: release $VERSION ($TARGETS)"
fi

if [ -z "$TARGETS" ]; then
  echo "Creating global release tag..."
  run_cmd git tag "v$VERSION" # Trigger all
  echo "🏷️  Created global tag: v$VERSION"
else
  IFS=',' read -ra ADDR <<< "$TARGETS"
  for i in "${ADDR[@]}"; do
    echo "Creating selective release tag for $i..."
    run_cmd git tag "$i-v$VERSION" # Trigger specific
    echo "🏷️  Created selective tag: $i-v$VERSION"
  done
fi

# 4. Push
current_branch=$(git branch --show-current)
echo "Pushing commit and tags to origin/$current_branch..."
run_cmd git push origin "$current_branch" --tags
echo "🎉 Release $VERSION dispatched to GitHub from branch $current_branch!"
