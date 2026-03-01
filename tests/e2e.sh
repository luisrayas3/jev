#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

ID="e2etest01"
PLAN_DIR="$ROOT/plans/$ID"

cleanup() {
    rm -rf "$PLAN_DIR"
}
trap cleanup EXIT

echo "=== e2e: scaffolding plan $ID ==="

mkdir -p "$PLAN_DIR/src"

cat > "$PLAN_DIR/Cargo.toml" <<EOF
[package]
name = "plan${ID}"
version = "0.1.0"
edition = "2021"

[workspace]

[dependencies]
jevs = { path = "$ROOT/jevs" }
jevsr = { path = "$ROOT/jevsr" }
tokio = { version = "1", features = ["full"] }
anyhow = "1"
EOF

ln -sf "$ROOT/plan_main.rs" "$PLAN_DIR/src/main.rs"

cat > "$PLAN_DIR/src/resources.rs" <<'EOF'
pub struct Resources {
    pub fs: jevs::File,
}

pub fn create() -> Resources {
    Resources {
        fs: jevsr::open_file("."),
    }
}
EOF

cat > "$PLAN_DIR/src/tasks.rs" <<'EOF'
use jevs::*;
use crate::resources::Resources;

pub async fn root(res: &mut Resources) -> anyhow::Result<()> {
    let files = res.fs.glob("*.toml").await?;
    for f in &files {
        println!("{f}");
    }
    Ok(())
}
EOF

echo "=== e2e: building plan ==="
cargo build --release --manifest-path "$PLAN_DIR/Cargo.toml"

echo "=== e2e: running plan ==="
OUTPUT=$("$PLAN_DIR/target/release/plan${ID}" 2>&1)

if echo "$OUTPUT" | grep -q "Cargo.toml"; then
    echo "=== e2e: PASSED ==="
else
    echo "=== e2e: FAILED ==="
    echo "Output was: $OUTPUT"
    exit 1
fi
