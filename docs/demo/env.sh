#!/usr/bin/env bash
# Sourced by demo.tape (hidden): regenerates the fixture environment and
# shapes a clean shell for recording. Prerequisite: cargo build --release.
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$REPO/target/release/skillhealth"
[ -x "$BIN" ] || {
  echo "missing $BIN — run: cargo build --release" >&2
  return 1
}

bash "$REPO/docs/demo/setup-demo.sh" >/dev/null

DEMO="/tmp/skillhealth-demo"
NOW="2026-06-10T17:00:00Z" # must match setup-demo.sh

skillhealth() {
  "$BIN" --config-dir "$DEMO/config" --projects-dir "$DEMO/projects" \
    --cache-dir "$DEMO/cache" --now "$NOW" "$@"
}

cd "$DEMO/repo"
PS1='\[\e[38;5;75m\]❯\[\e[0m\] '
