# lean4_split — Native Lean4 Declaration Splitter

Splits mathlib into one file per declaration using the Lean4 kernel
`Environment` API. Unlike regex-based approaches, this gives **exact**
dependency tracking via `Expr.foldConsts`.

## Why?

- **Reduce compile times**: only rebuild the transitive closure of what changed
- **Parallel builds**: independent branches of the DAG compile in parallel
- **Cherry-pick imports**: `import Split.HVertexOperator` pulls only what's needed
- **Exact deps**: no false positives from string matching

## Usage

```bash
# From mathlib root, after `lake build`:
lake env lean tools/lean4_split/SplitDecls.lean -- \
  Mathlib.Algebra.Vertex.HVertexOperator ./split-out
```

## Output

```
split-out/
├── Split/
│   ├── HVertexOperator.lean          # one file per decl
│   ├── HahnModule_single.lean
│   ├── ...
├── dag.json                          # exact dependency graph
└── lakefile.toml                     # Lake project config
```

## How it works

1. `importModules` loads the target module's full environment
2. `env.constants` enumerates every declaration
3. `Expr.foldConsts` on type + value extracts exact references
4. BFS closure from root → minimal set of needed declarations
5. Topological sort by dependency
6. Emit one `.lean` file per declaration with `import` statements

## Comparison

| | Rust (`lean_split.rs`) | Lean4 (`SplitDecls.lean`) |
|---|---|---|
| Dependency tracking | Regex on source | Kernel `Expr.foldConsts` |
| Handles elaborated terms | No | Yes |
| Handles instances/TC | Name matching | Exact |
| Handles notation/macros | No (pre-elab) | Yes (post-elab) |
| Requires `lake build` | No | Yes (needs .olean) |
| Speed | Fast | Slower (loads env) |

The Rust tool is better for quick file-level splitting. The Lean4 tool
is better for precise declaration-level splitting where correctness matters.
