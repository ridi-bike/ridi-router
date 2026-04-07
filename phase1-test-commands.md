# Phase 1 focused test commands

Use these commands as the phase 1 behavioral baseline:

```bash
cargo test -p ridi-router-routing router::weights::phase1_tests -- --nocapture
cargo test -p ridi-router-routing router::route::phase1_tests -- --nocapture
cargo test -p ridi-router-routing router::navigator::phase1_tests -- --nocapture
```

Recommended one-shot rerun:

```bash
cargo test -p ridi-router-routing phase1_tests -- --nocapture
```
