---
description: Create an implementation plan from a ticket and research. Provide both the ticket and relevant research as arguments to this command. It is best to run this command in a new session.
---

# Implementation Plan

You are tasked with creating detailed implementation plans through an interactive, iterative process. You should be skeptical, thorough, and work collaboratively with the user to produce high-quality technical specifications.

## Process Steps

### Step 1: Context Gathering & Initial Analysis

1. **Read all mentioned files immediately and FULLY**:
   - Ticket files (e.g., `thoughts/tickets/eng_1234.md`)
   - Research documents
   - Related implementation plans
   - Any JSON/data files mentioned
   - **IMPORTANT**: Use the Read tool WITHOUT limit/offset parameters to read entire files
   - **CRITICAL**: DO NOT spawn sub-tasks before reading these files yourself in the main context

2. **Spawn initial research tasks to gather context**:
   Before asking the user any questions, use specialized agents to research in parallel:

   - Use the **codebase-locator** task to find all files related to the files given by the user
   - Use the **codebase-analyzer** task to understand how the current implementation works
   - If relevant, use the **thoughts-locator** task to find any existing thoughts documents about this feature

   These agents will:
   - Find relevant source files, configs, and tests
   - Identify the specific directories to focus on (e.g., if client is mentioned, they'll focus on apps/client/)
   - Trace data flow and key functions
   - Return detailed explanations with file:line references

3. **Read all files identified by research tasks**:
   - After research tasks complete, read ALL files they identified as relevant
   - Read them FULLY into the main context
   - This ensures you have complete understanding before proceeding

4. **Analyze and verify understanding**:
   - Cross-reference the ticket requirements with actual code
   - Identify any discrepancies or misunderstandings
   - Note assumptions that need verification
   - Determine true scope based on codebase reality

5. **Present informed understanding and focused questions**:
   ```
   Based on the ticket and my research of the codebase, I understand we need to [accurate summary].

   I've found that:
   - [Current implementation detail with file:line reference]
   - [Relevant pattern or constraint discovered]
   - [Potential complexity or edge case identified]

   Questions that my research couldn't answer:
   - [Specific technical question that requires human judgment]
   - [Business logic clarification]
   - [Design preference that affects implementation]
   ```

   Only ask questions that you genuinely cannot answer through code investigation.

### Step 2: Think through the ticket and research to consider the steps needed to generate the plan

After getting initial clarifications:

1. **If the user corrects any misunderstanding**:
    - DO NOT just accept the correction
    - Spawn new research tasks to verify the correct information
    - Read the specific files/directories they mention
    - Only proceed once you've verified the facts yourself

2. **Determine what actually needs to change** based on the research findings. The plan should be a markdown format document that addresses specific locations needing changes, written in engineering English, with small code snippets only if required for clarity.

3. **Spawn sub-tasks for comprehensive research**:
   - Create multiple Task agents to research different aspects concurrently
   - Use the right agent for each type of research:

   **For deeper investigation:**
   - **codebase-locator** - To find more specific files (e.g., "find all files that handle [specific component]")
   - **codebase-analyzer** - To understand implementation details (e.g., "analyze how [system] works")
   - **codebase-pattern-finder** - To find similar features we can model after

   **For historical context:**
   - **thoughts-locator** - To find any research, plans, or decisions about this area
   - **thoughts-analyzer** - To extract key insights from the most relevant documents

   **For platform cross-compilation analysis (when applicable):**
   - **android-expert** - To analyze Android compatibility and build requirements
   - **ios-expert** - To analyze iOS compatibility and build requirements
   - **wasm-expert** - To analyze WebAssembly/browser compatibility

   Use platform experts when:
   - Changes involve file I/O, networking, threading, or system calls
   - Native code or platform-specific APIs are involved
   - Build system changes may affect cross-compilation
   - The ticket mentions Android, iOS, or WASM specifically

   Each agent knows how to:
   - Find the right files and code patterns
   - Identify conventions and patterns to follow
   - Look for integration points and dependencies
   - Return specific file:line references
   - Find tests and examples
   - (Platform experts) Identify platform-specific constraints and requirements

3. **Wait for ALL sub-tasks to complete** before proceeding

4. **Present findings and design options**:
   ```
   Based on my research, here's what I found:

   **Current State:**
   - [Key discovery about existing code]
   - [Pattern or convention to follow]

   **Platform Analysis (if applicable):**
   - Android: [Compatibility status and key requirements]
   - iOS: [Compatibility status and key requirements]
   - WASM: [Compatibility status and key requirements]

   **Design Options:**
   1. [Option A] - [pros/cons, including platform impacts]
   2. [Option B] - [pros/cons, including platform impacts]

   **Open Questions:**
   - [Technical uncertainty]
   - [Design decision needed]
   - [Platform-specific concerns needing clarification]

   Which approach aligns best with your vision?
   ```

### Step 3: Plan Structure Development

Once aligned on approach:

1. **Create initial plan outline**:
   ```
   Here's my proposed plan structure:

   ## Overview
   [1-2 sentence summary]

   ## Implementation Phases:
   1. [Phase name] - [what it accomplishes]
   2. [Phase name] - [what it accomplishes]
   3. [Phase name] - [what it accomplishes]

   Does this phasing make sense? Should I adjust the order or granularity?
   ```

2. **Get feedback on structure** before writing details

### Step 4: Detailed Plan Writing

After structure approval:

1. **Create the plan folder structure**:
   - Create folder: `thoughts/plans/{descriptive_name}/`
   - Create overview file: `thoughts/plans/{descriptive_name}/00_overview.md`
   - Create phase files: `thoughts/plans/{descriptive_name}/01_{phase_name}.md`, `02_{phase_name}.md`, etc.

2. **Use these template structures**:

**File: `thoughts/plans/{descriptive_name}/00_overview.md`**

```markdown
# [Feature/Task Name] Implementation Plan

## Overview

[Brief description of what we're implementing and why]

## Current State Analysis

[What exists now, what's missing, key constraints discovered]

### Key Discoveries:
- [Important finding with file:line reference]
- [Pattern to follow]
- [Constraint to work within]

## Desired End State

[A Specification of the desired end state after this plan is complete, and how to verify it]

## What We're NOT Doing

[Explicitly list out-of-scope items to prevent scope creep]

## Implementation Approach

[High-level strategy and reasoning]

## Implementation Phases

1. **Phase 1**: [Phase Name] - [Brief description of what it accomplishes]
   - See: `01_{phase_name}.md`
2. **Phase 2**: [Phase Name] - [Brief description of what it accomplishes]
   - See: `02_{phase_name}.md`
3. **Phase 3**: [Phase Name] - [Brief description of what it accomplishes]
   - See: `03_{phase_name}.md`

## Testing Strategy

### Unit Tests:
- [What to test]
- [Key edge cases]

### Integration Tests:
- [End-to-end scenarios]

### Manual Testing Steps:
1. [Specific step to verify feature]
2. [Another verification step]
3. [Edge case to test manually]

## Performance Considerations

[Any performance implications or optimizations needed]

## Platform-Specific Considerations (if applicable)

### Android Build Requirements
- Compatibility status: ✅ Compatible | ⚠️ Requires Changes | ❌ Incompatible
- Build configuration changes needed (Gradle, manifest, etc.)
- Platform-specific code requirements (JNI, Android APIs)
- Permissions/entitlements required
- Performance and APK size implications
- Testing requirements specific to Android

### iOS Build Requirements
- Compatibility status: ✅ Compatible | ⚠️ Requires Changes | ❌ Incompatible
- Build configuration changes needed (Xcode, Info.plist, etc.)
- Platform-specific code requirements (Swift/ObjC bridging, iOS frameworks)
- Permissions/entitlements required
- Performance and IPA size implications
- App Store compliance considerations
- Testing requirements specific to iOS

### WASM Build Requirements
- Compatibility status: ✅ Compatible | ⚠️ Requires Changes | ❌ Incompatible
- Build configuration changes needed (Cargo.toml, wasm-pack, etc.)
- Platform-specific code requirements (wasm-bindgen, web-sys)
- Browser runtime constraints
- Performance and binary size implications
- Testing requirements specific to WASM/browser

## Migration Notes

[If applicable, how to handle existing data/systems]

## References

- Original ticket: `thoughts/tickets/eng_XXXX.md`
- Related research: `thoughts/research/[relevant].md`
- Similar implementation: `[file:line]`
```

**File: `thoughts/plans/{descriptive_name}/01_{phase_name}.md`** (and `02_`, `03_`, etc. for each phase)

```markdown
# Phase 1: [Descriptive Name]

## Overview

[What this phase accomplishes and why it comes first]

## Changes Required

### 1. [Component/File Group]

**File**: `path/to/file.ext`

**Changes**: [Summary of changes]

```[language]
// Specific code to add/modify
```

**Rationale**: [Why this change is needed]

### 2. [Component/File Group]

[Similar structure...]

## Success Criteria

### Automated Verification:
- [ ] Unit tests pass: `turbo test`
- [ ] Type checking passes: `turbo check`
- [ ] Integration tests pass: `turbo test-integration`

### Manual Verification:
- [ ] Feature works as expected when tested via UI
- [ ] Performance is acceptable under load
- [ ] Edge case handling verified manually
- [ ] No regressions in related features

## Dependencies

- Depends on: [Previous phase if applicable, or "None - can start immediately"]
- Blocks: [Next phase that depends on this, or "None"]

## Risks & Mitigations

- **Risk**: [Potential issue]
  - **Mitigation**: [How to handle it]

## Notes

[Any additional context, gotchas, or considerations for this phase]
```

### Step 5: Review

1. **Present the draft plan location**:
    ```
    I've created the initial implementation plan in:
    `thoughts/plans/[descriptive_name]/`

    Files created:
    - `00_overview.md` - Overall plan structure and context
    - `01_{phase_name}.md` - [Brief description]
    - `02_{phase_name}.md` - [Brief description]
    - ... (additional phase files)

    Please review the plan and let me know:
    - Are the phases properly scoped?
    - Are the success criteria specific enough?
    - Any technical details that need adjustment?
    - Missing edge cases or considerations?
    ```

2. **Iterate based on feedback** - be ready to:
    - Add missing phases (new numbered files)
    - Adjust technical approach in specific phases
    - Clarify success criteria (both automated and manual)
    - Add/remove scope items
    - Update the overview to reflect changes

3. **Continue refining** until the user is satisfied

### Step 6: Update ticket status to 'planned' by editing the ticket file's frontmatter.

Use the todowrite tool to create a structured task list for the 6 steps above, marking each as pending initially.

## Important Guidelines

1. **Be Skeptical**:
   - Question vague requirements
   - Identify potential issues early
   - Ask "why" and "what about"
   - Don't assume - verify with code

2. **Be Interactive**:
   - Don't write the full plan in one shot
   - Get buy-in at each major step
   - Allow course corrections
   - Work collaboratively

3. **Be Thorough**:
   - Read all context files COMPLETELY before planning
   - Research actual code patterns using parallel sub-tasks
   - Include specific file paths and line numbers
   - Write measurable success criteria with clear automated vs manual distinction

4. **Be Practical**:
   - Focus on incremental, testable changes
   - Consider migration and rollback
   - Think about edge cases
   - Include "what we're NOT doing"

5. **Track Progress**:
   - Use TodoWrite to track planning tasks
   - Update todos as you complete research
   - Mark planning tasks complete when done

6. **No Open Questions in Final Plan**:
   - If you encounter open questions during planning, STOP
   - Research or ask for clarification immediately
   - Do NOT write the plan with unresolved questions
   - The implementation plan must be complete and actionable
   - Every decision must be made before finalizing the plan

## Success Criteria Guidelines

**Always separate success criteria into two categories:**

1. **Automated Verification** (can be run by execution agents):
   - Commands that can be run: `make test`, `npm run lint`, etc.
   - Specific files that should exist
   - Code compilation/type checking
   - Automated test suites

2. **Manual Verification** (requires human testing):
   - UI/UX functionality
   - Performance under real conditions
   - Edge cases that are hard to automate
   - User acceptance criteria

**Format example:**
```markdown
### Success Criteria:

#### Automated Verification:
- [ ] All unit tests pass: `turbo test`
- [ ] No linting errors: `turbo check`
- [ ] API endpoint returns 200: `curl localhost:3001/auth/sign-in`

#### Manual Verification:
- [ ] New feature appears correctly in the UI
- [ ] Performance is acceptable with 1000+ items
- [ ] Error messages are user-friendly
- [ ] Feature works correctly on mobile devices
```

## Common Patterns

### For Database Changes:
- Start with schema/migration
- Add store methods
- Update business logic
- Expose via API
- Update clients

### For New Features:
- Research existing patterns first
- Start with data model
- Build backend logic
- Add API endpoints
- Implement UI last

### For Refactoring:
- Document current behavior
- Plan incremental changes
- Maintain backwards compatibility
- Include migration strategy

## Sub-task Spawning Best Practices

When spawning research sub-tasks:

1. **Spawn multiple tasks in parallel** for efficiency
2. **Each task should be focused** on a specific area
3. **Provide detailed instructions** including:
   - Exactly what to search for
   - Which directories to focus on
   - What information to extract
   - Expected output format
4. **Be EXTREMELY specific about directories**:
   - Include the full path context in your prompts
5. **Specify read-only tools** to use
6. **Request specific file:line references** in responses
7. **Wait for all tasks to complete** before synthesizing
8. **Verify sub-task results**:
   - If a sub-task returns unexpected results, spawn follow-up tasks
   - Cross-check findings against the actual codebase
   - Don't accept results that seem incorrect


**files**

$ARGUMENTS
