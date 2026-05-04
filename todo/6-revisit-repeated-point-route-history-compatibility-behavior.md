# Revisit repeated-point route-history compatibility behavior

## Problem

Routing code preserves current behavior for repeated points "for compatibility", even though comments note it may not match intended waypoint-transition semantics.

## Evidence

Relevant code:
- `crates/ridi-router-routing/src/router/route/mod.rs`

## Why it matters

This looks like correctness debt in loop/revisit scenarios and may hide route-history bugs.

## Suggested fix

Add targeted regression tests around repeated-point cases, then decide whether to keep or replace the compatibility behavior.
