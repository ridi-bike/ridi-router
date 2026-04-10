i am investigating performance.
see the full review in ./perf.md
i want to focus on | `walker::move_forward_to_next_fork_with_context` | 357,872 | **13.70 s** | **27.09%** | biggest direct traversal hotspot |
please examine the functionality and suggest ways how to improve the performance. 
write your findings to ./perf-plan.md, overwrite the file 
lets discuss the possible improvements together 


