# lean_split

Rust tool for splitting mathlib4 into per-module content-addressed packages.

## Build

```bash
cargo build --release
```

## Usage

```bash
# List all transitive imports for a module
lean_split closure Mathlib.Algebra.Vertex.HVertexOperator --packages-dir .lake/packages

# Generate per-module catalog (JSON)
lean_split catalog Mathlib.Algebra.Vertex.HVertexOperator --packages-dir .lake/packages --output catalog/

# Generate per-module nix derivations
lean_split nix Mathlib.Algebra.Vertex.HVertexOperator --packages-dir .lake/packages --output nix-modules/

# Pick modules to build
nix-build nix-modules/select.nix --arg wanted '["Mathlib_Algebra_Vertex_HVertexOperator"]'
```

## Subcommands

| Command   | Description |
|-----------|-------------|
| `closure` | Transitive import closure (flat list) |
| `dag`     | Import DAG as JSON |
| `decls`   | Content-hash every declaration |
| `extract` | Extract minimal declaration subset by hash |
| `catalog` | Per-module JSON with hash, imports, size |
| `nix`     | Per-file nix derivations + select.nix |
