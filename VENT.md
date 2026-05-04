# VENT

Feedback log. Repeated/systemic workflow friction that should become future automation, docs, or workflow fixes.

## 26-05-04 22:28 — tool_error

The installed `osmium` binary failed to start because `libboost_program_options.so.1.89.0` is missing. I worked around it by generating the tiny OSM PBF fixture directly with a Python protobuf encoder. Keeping osmium's runtime libraries in sync, or documenting a supported fixture-generation command/container, would avoid this manual workaround next time.
