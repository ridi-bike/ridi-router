i am investigating performance.
see the full review in ./perf.md
i want to focus on | `graph::get_adjacent` | **38.30 s** | adjacency lookup stack was already large |
please examine the functionality and suggest ways how to improve the performance. 
write your findings to ./perf-plan.md, overwrite the file 
lets discuss the possible improvements together 


i am investigating performance.
please run perf profiling with hotpath by running `RIDI_FEATURES ./dev.sh riga,latvia sigulda,latvia`
write a full review in ./perf.md, overwrite existing contents
give me a summary of your findings
you can use the hotpath MCP while the process is running
