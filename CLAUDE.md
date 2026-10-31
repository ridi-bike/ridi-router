# CLAUDE.md

## Memory limits for heavy jobs

This machine has 23 GB RAM. When a process exhausts memory, systemd-oomd kills the
entire login scope, which includes `herdr server` and every Claude Code session on
the machine, not just the offending job.

- Run any heavy job (tile generation, `rmdf-diff`, benchmarks over `map-data/`) in its
  own capped cgroup so only that job gets killed:

  ```sh
  systemd-run --user --scope -p MemoryMax=16G -p MemorySwapMax=8G <command>
  ```

- Do not run anything on the full Europe or world datasets (`map-data/input-europe`,
  `map-data/input-world`, `out/*-europe`) without asking the user first.
- Prefer streaming/chunked processing over collecting whole datasets in memory.
  `rmdf-diff --digest` once held every decoded tile model and grew to ~136 GB of swap
  (fixed in 153042f by hashing and dropping each tile on its worker).
