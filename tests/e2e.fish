#!/usr/bin/env fish

set -l root (realpath (dirname (status filename))/..)
set -l fixtures $root/tests/fixtures/e2e

echo "=== e2e: full pipeline test ==="

cd $fixtures

set -l output (echo y | env (cat $root/.env) \
    cargo run --manifest-path $root/Cargo.toml \
    --bin jev -- go \
    "read hello.txt and print its contents exactly" 2>/dev/null)

# Find and clean up the generated plan directory
set -l plan_dir (ls -dt $root/plans/*/ 2>/dev/null | head -1)
if test -n "$plan_dir"
  rm -rf $plan_dir
end

if string match -q "*jev-e2e-ok*" -- $output
  echo "=== e2e: PASSED ==="
else
  echo "=== e2e: FAILED ==="
  echo "Output was: $output"
  exit 1
end
