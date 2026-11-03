# VENT

Feedback log. Repeated/systemic workflow friction that should become future automation, docs, or workflow fixes.

## 26-06-01 11:13 — ts-node expression validation

Repeated attempts to run a small ts-node runtime validation script failed because the project/global ts-node picked up incompatible TS 6 config defaults/rootDir/moduleResolution settings. Workaround was manual expression inspection plus normal lint/typecheck. A documented project-local way to run one-off TypeScript scripts, or a package script using the same Metro/tsconfig setup, would prevent this retry loop.
