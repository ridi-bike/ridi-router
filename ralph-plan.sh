#!/bin/bash

# Script to execute plan files incrementally

# Check if directory argument is provided
if [ -z "$1" ]; then
    echo "Usage: $0 <plan-directory>"
    echo "Example: $0 thoughts/plans/incremental_current_day_normalization"
    exit 1
fi

PLAN_DIR="$1"

# Check if plan directory exists
if [ ! -d "$PLAN_DIR" ]; then
    echo "Error: Plan directory not found: $PLAN_DIR"
    exit 1
fi

# Get all .md files sorted by name into an array
FILES=($(ls "$PLAN_DIR"/*.md 2>/dev/null | sort))

if [ ${#FILES[@]} -eq 0 ]; then
    echo "Error: No .md files found in $PLAN_DIR"
    exit 1
fi

# First file should be 00_overview.md
OVERVIEW="${FILES[0]}"

# Execute incrementally: 00+01, 00+02, 00+03, etc.
for i in $(seq 1 $((${#FILES[@]} - 1))); do
    CURRENT_FILE="${FILES[$i]}"
    echo "=========================================="
    echo "Iteration $i: Executing ${OVERVIEW} and ${CURRENT_FILE}"
    echo "=========================================="
    claude -p "/execute $OVERVIEW $CURRENT_FILE" --dangerously-skip-permissions

    echo ""
    echo "Committing changes for phase $i..."
    git add .
    git commit -m "wip: phase $i"

done

echo "All iterations completed!"

