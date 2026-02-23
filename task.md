I get a warning when I compile.
please investigate and tell me where this may have been used in the past and why it might not be used anymore. look at git history

```
warning: value assigned to `offset` is never read
  --> src/rmdf/generator/writer.rs:79:9
   |
79 |         offset += rules.len();
   |         ^^^^^^
   |
   = help: maybe it is overwritten before being read?
   = note: `#[warn(unused_assignments)]` on by default

warning: unused variable: `combined_bounds`
   --> src/rmdf/generator/mod.rs:770:13
    |
770 |         let combined_bounds = self.calculate_combined_bounds(&all_bounds);
    |             ^^^^^^^^^^^^^^^ help: if this is intentional, prefix it with an underscore: `_combined_bounds`
```
