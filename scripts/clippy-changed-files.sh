#!/bin/bash
# Clippy gate for lefthook pre-commit.
#
# Runs clippy on the workspace (warnings elsewhere are pre-existing baseline),
# but fails only when a warning or error points at one of the staged .rs
# files. Usage: scripts/clippy-changed-files.sh <staged files...>
set -u

files=("$@")
[ ${#files[@]} -eq 0 ] && exit 0

report=$(mktemp -t clippy-changed-files)
trap 'rm -f "$report"' EXIT

cargo +nightly clippy --workspace --all-targets --message-format=json 2>/dev/null \
    | jq -r --arg pwd "$PWD/" '
        select(.reason == "compiler-message")
        | .message
        | select(.level == "warning" or .level == "error")
        | .spans[]?
        | select(.is_primary)
        | (.file_name | ltrimstr($pwd)) // .file_name' \
    | sort -u > "$report"

status=0
for f in "${files[@]}"; do
    if grep -qxF "$f" "$report"; then
        echo "❌ clippy reports issues in staged file: $f"
        echo "   run 'cargo clippy --workspace --all-targets' for details"
        status=1
    fi
done

exit $status
