#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

PI_BIN="${PI_BIN:-pi}"
FORCE=0
MODEL="${MODEL:-}"
THINKING="${THINKING:-medium}"
PARALLEL="${PARALLEL:-1}"

usage() {
    cat <<'EOF'
Usage: ./ralph-todo.sh [--force] [--model <model>] [--thinking <level>] [--parallel <n>] [--pi-bin <path>]

Loops through all ./todo-*.md files (excluding ./todo-review-*.md) and asks pi to:
- analyze whether the todo is still relevant
- inspect code/context
- write a review doc next to it as ./todo-review-<name>.md

Options:
  --force              Regenerate review files even if they already exist
  --model <model>      Pass a specific model to pi
  --thinking <level>   Thinking level for pi (default: medium)
  --parallel <n>       Run up to n pi jobs in parallel (default: 1)
  -j <n>               Short form of --parallel
  --pi-bin <path>      pi executable to use (default: pi)
  -h, --help           Show this help

Environment overrides:
  PI_BIN               Alternative pi executable
  MODEL                Default model
  THINKING             Default thinking level
  PARALLEL             Default parallelism
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --force)
            FORCE=1
            shift
            ;;
        --model)
            MODEL="${2:?missing value for --model}"
            shift 2
            ;;
        --thinking)
            THINKING="${2:?missing value for --thinking}"
            shift 2
            ;;
        --parallel|-j)
            PARALLEL="${2:?missing value for --parallel}"
            shift 2
            ;;
        --pi-bin)
            PI_BIN="${2:?missing value for --pi-bin}"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

if ! [[ "$PARALLEL" =~ ^[1-9][0-9]*$ ]]; then
    echo "Error: --parallel must be a positive integer" >&2
    exit 1
fi

if ! command -v "$PI_BIN" >/dev/null 2>&1; then
    echo "Error: pi executable not found: $PI_BIN" >&2
    exit 1
fi

mapfile -t TODO_FILES < <(find . -maxdepth 1 -type f -name 'todo-*.md' ! -name 'todo-review-*.md' -printf '%f\n' | sort)

if [[ ${#TODO_FILES[@]} -eq 0 ]]; then
    echo "No todo-*.md files found."
    exit 0
fi

build_prompt() {
    local todo_file="$1"
    local review_file="$2"

    cat <<EOF
Analyze @./$todo_file.

Examine the todo file, inspect the relevant repository context, and decide whether this todo is still relevant, already covered by other functionality, or obsolete.

Write the result to ./$review_file.

Requirements:
- Do not edit any existing source files.
- Create exactly one review doc at ./$review_file.
- The review should be the starting point for a later implementation plan.
- Be evidence-based and cite concrete file paths when relevant.
- Keep the write-up practical, not speculative.

Please structure the review roughly like this:
1. Title
2. Verdict: relevant / partially relevant / already covered / obsolete
3. Summary
4. Current evidence from the codebase
5. If obsolete or already covered: explain why, what replaced it, and any residual caveats
6. If still relevant:
   - the actual problem
   - what behavior is missing or risky
   - possible solution options
   - tradeoffs / implications
   - recommended direction
   - open questions
   - suggested implementation starting point
7. Related files to inspect next

Important:
- Check whether the todo is superseded by later refactors or existing behavior.
- If the todo overlaps with another mechanism, explain the overlap clearly.
- If the todo text is stale or partially wrong, say so explicitly.
EOF
}

run_one() {
    local todo_file="$1"
    local stem="${todo_file%.md}"
    local short_name="${stem#todo-}"
    local review_file="todo-review-${short_name}.md"
    local prompt
    prompt="$(build_prompt "$todo_file" "$review_file")"

    if [[ -f "$review_file" && "$FORCE" -ne 1 ]]; then
        echo "[skip] $review_file already exists"
        return 0
    fi

    echo "[run ] $todo_file -> $review_file"

    local -a cmd
    cmd=("$PI_BIN" -p --no-session --tools read,bash,write --thinking "$THINKING")

    if [[ -n "$MODEL" ]]; then
        cmd+=(--model "$MODEL")
    fi

    cmd+=("@./$todo_file" "$prompt")

    if "${cmd[@]}"; then
        echo "[ ok ] $review_file"
        return 0
    else
        echo "[fail] $todo_file" >&2
        return 1
    fi
}

run_all_sequential() {
    local failures=0
    local todo_file

    for todo_file in "${TODO_FILES[@]}"; do
        if ! run_one "$todo_file"; then
            failures=$((failures + 1))
        fi
    done

    echo "$failures"
}

run_all_parallel() {
    local status_dir
    status_dir="$(mktemp -d)"
    trap 'rm -rf "$status_dir"' RETURN

    local active=0
    local idx=0
    local todo_file

    for todo_file in "${TODO_FILES[@]}"; do
        idx=$((idx + 1))
        (
            if run_one "$todo_file"; then
                echo 0 > "$status_dir/$idx.status"
            else
                echo 1 > "$status_dir/$idx.status"
            fi
        ) &

        active=$((active + 1))

        if [[ "$active" -ge "$PARALLEL" ]]; then
            if ! wait -n; then
                :
            fi
            active=$((active - 1))
        fi
    done

    while [[ "$active" -gt 0 ]]; do
        if ! wait -n; then
            :
        fi
        active=$((active - 1))
    done

    local failures=0
    local status_file
    shopt -s nullglob
    for status_file in "$status_dir"/*.status; do
        if [[ "$(<"$status_file")" != "0" ]]; then
            failures=$((failures + 1))
        fi
    done
    shopt -u nullglob

    echo "$failures"
}

echo "Found ${#TODO_FILES[@]} todo file(s); parallel=$PARALLEL"

if [[ "$PARALLEL" -eq 1 ]]; then
    failures="$(run_all_sequential)"
else
    failures="$(run_all_parallel)"
fi

if [[ "$failures" -gt 0 ]]; then
    echo
    echo "Completed with $failures failure(s)." >&2
    exit 1
fi

echo
echo "Completed successfully for ${#TODO_FILES[@]} todo file(s)."
