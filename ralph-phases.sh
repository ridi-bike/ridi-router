#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

PI_BIN="${PI_BIN:-pi}"
MODEL="${MODEL:-}"
THINKING="${THINKING:-medium}"
START_AT="${START_AT:-}"
END_AT="${END_AT:-}"

usage() {
    cat <<'EOF'
Usage: ./ralph-phases.sh [--model <model>] [--thinking <level>] [--start-at <n>] [--end-at <n>] [--pi-bin <path>]

Loops through ./impl-phase-[num].md files in numeric order and asks pi to
implement each phase against ./perf-plan.md.

Also accepts legacy ./perf-plan-phase-[num].md files.

After each successful phase run, the script creates a git commit for that phase.

Options:
  --model <model>      Pass a specific model to pi
  --thinking <level>   Thinking level for pi (default: medium)
  --start-at <n>       Start at phase number n
  --end-at <n>         Stop after phase number n
  --pi-bin <path>      pi executable to use (default: pi)
  -h, --help           Show this help

Environment overrides:
  PI_BIN               Alternative pi executable
  MODEL                Default model
  THINKING             Default thinking level
  START_AT             Default starting phase
  END_AT               Default ending phase
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --model)
            MODEL="${2:?missing value for --model}"
            shift 2
            ;;
        --thinking)
            THINKING="${2:?missing value for --thinking}"
            shift 2
            ;;
        --start-at)
            START_AT="${2:?missing value for --start-at}"
            shift 2
            ;;
        --end-at)
            END_AT="${2:?missing value for --end-at}"
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

if [[ -n "$START_AT" && ! "$START_AT" =~ ^[1-9][0-9]*$ ]]; then
    echo "Error: --start-at must be a positive integer" >&2
    exit 1
fi

if [[ -n "$END_AT" && ! "$END_AT" =~ ^[1-9][0-9]*$ ]]; then
    echo "Error: --end-at must be a positive integer" >&2
    exit 1
fi

if [[ -n "$START_AT" && -n "$END_AT" && "$START_AT" -gt "$END_AT" ]]; then
    echo "Error: --start-at cannot be greater than --end-at" >&2
    exit 1
fi

if ! command -v "$PI_BIN" >/dev/null 2>&1; then
    echo "Error: pi executable not found: $PI_BIN" >&2
    exit 1
fi

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    echo "Error: this script must be run inside a git repository" >&2
    exit 1
fi

if [[ ! -f ./perf-plan.md ]]; then
    echo "Error: ./perf-plan.md not found" >&2
    exit 1
fi

extract_phase_num() {
    local phase_file="$1"

    if [[ ! "$phase_file" =~ ^(impl-phase|perf-plan-phase)-([0-9]+)\.md$ ]]; then
        echo "Error: unexpected phase filename: $phase_file" >&2
        return 1
    fi

    printf '%d\n' "$((10#${BASH_REMATCH[2]}))"
}

ensure_tracked_clean() {
    if ! git diff --quiet || ! git diff --cached --quiet; then
        echo "Error: working tree has tracked changes. Commit or stash them first." >&2
        git status --short >&2 || true
        exit 1
    fi
}

capture_untracked() {
    local -n out_ref="$1"
    out_ref=()

    while IFS= read -r -d '' path; do
        out_ref+=("$path")
    done < <(git ls-files --others --exclude-standard -z)
}

stage_new_untracked() {
    local -n before_ref="$1"
    local -A seen=()
    local path

    for path in "${before_ref[@]}"; do
        seen["$path"]=1
    done

    while IFS= read -r -d '' path; do
        if [[ -z "${seen["$path"]+x}" ]]; then
            git add -- "$path"
        fi
    done < <(git ls-files --others --exclude-standard -z)
}

build_prompt() {
    local phase_file="$1"
    local phase_num="$2"

    cat <<EOF
Please read @./perf-plan.md.
Implement ONLY phase $phase_num in @./$phase_file.

Requirements:
- Do not work on later phases.
- Make only the code, test, and documentation changes needed for this phase.
- Run relevant tests/checks for the touched code before finishing.
- Do not create a git commit yourself; the wrapper script will commit after you finish.
- If you discover follow-up work for later phases, leave it for the later phase docs.

When finished, stop.
EOF
}

run_one() {
    local phase_file="$1"
    local phase_num
    phase_num="$(extract_phase_num "$phase_file")"

    ensure_tracked_clean

    local -a baseline_untracked=()
    capture_untracked baseline_untracked

    local prompt
    prompt="$(build_prompt "$phase_file" "$phase_num")"

    echo "[run ] phase $phase_num ($phase_file)"

    local -a cmd
    cmd=("$PI_BIN" -p --no-session --tools read,bash,edit,write,lsp --thinking "$THINKING")

    if [[ -n "$MODEL" ]]; then
        cmd+=(--model "$MODEL")
    fi

    cmd+=("@./perf-plan.md" "@./$phase_file" "$prompt")

    if ! "${cmd[@]}"; then
        echo "[fail] phase $phase_num ($phase_file)" >&2
        git status --short >&2 || true
        return 1
    fi

    git add -u
    stage_new_untracked baseline_untracked

    if git diff --cached --quiet; then
        echo "[fail] phase $phase_num produced no committable changes" >&2
        git status --short >&2 || true
        return 1
    fi

    git commit -m "Implement phase $phase_num"

    echo "[ ok ] phase $phase_num committed"
}

shopt -s nullglob
ALL_PHASE_FILES=( ./impl-phase-*.md )
if [[ ${#ALL_PHASE_FILES[@]} -eq 0 ]]; then
    ALL_PHASE_FILES=( ./perf-plan-phase-*.md )
fi
shopt -u nullglob

for i in "${!ALL_PHASE_FILES[@]}"; do
    ALL_PHASE_FILES[$i]="${ALL_PHASE_FILES[$i]#./}"
done

if [[ ${#ALL_PHASE_FILES[@]} -gt 1 ]]; then
    mapfile -t ALL_PHASE_FILES < <(printf '%s\n' "${ALL_PHASE_FILES[@]}" | sort -V)
fi

if [[ ${#ALL_PHASE_FILES[@]} -eq 0 ]]; then
    echo "No impl-phase-*.md or perf-plan-phase-*.md files found."
    exit 0
fi

PHASE_FILES=()
for phase_file in "${ALL_PHASE_FILES[@]}"; do
    phase_num="$(extract_phase_num "$phase_file")"

    if [[ -n "$START_AT" && "$phase_num" -lt "$START_AT" ]]; then
        continue
    fi

    if [[ -n "$END_AT" && "$phase_num" -gt "$END_AT" ]]; then
        continue
    fi

    PHASE_FILES+=("$phase_file")
done

if [[ ${#PHASE_FILES[@]} -eq 0 ]]; then
    echo "No phase files matched the requested range."
    exit 0
fi

echo "Found ${#PHASE_FILES[@]} phase file(s)."

for phase_file in "${PHASE_FILES[@]}"; do
    if ! run_one "$phase_file"; then
        echo
        echo "Stopped after failure in $phase_file." >&2
        exit 1
    fi
done

echo
echo "Completed successfully for ${#PHASE_FILES[@]} phase(s)."
