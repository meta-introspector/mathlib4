# lean_split

Rust tool for splitting mathlib4 into per-module content-addressed packages.
Any user can generate a nix flake that builds only the modules they need.

## Quick Start (via mmgroup-rust)

```bash
# Clone mmgroup-rust with mathlib4 as submodule
git clone --recurse-submodules https://github.com/meta-introspector/mmgroup-rust
cd mmgroup-rust

# Build lean_split and generate all outputs
make split-all

# Outputs in staging/:
#   closure.txt          — module list
#   mathlib-catalog/     — per-module JSON
#   mathlib-nix/         — per-file nix derivations + flake.nix
```

## Build Standalone

```bash
cargo build --release
```

## Usage

```bash
# List all transitive imports for a module
lean_split closure Mathlib.Algebra.Vertex.HVertexOperator --packages-dir .lake/packages

# Generate per-module catalog (JSON)
lean_split catalog Mathlib.Algebra.Vertex.HVertexOperator --packages-dir .lake/packages --output catalog/

# Generate per-module nix derivations + flake.nix
lean_split nix Mathlib.Algebra.Vertex.HVertexOperator --packages-dir .lake/packages --output my-subset/

# Build only what you need
cd my-subset && nix build .#Mathlib_Algebra_Vertex_HVertexOperator
```

## Pick Your Own Modules

Replace the root module with whatever you need:

```bash
lean_split nix Mathlib.Topology.Basic --packages-dir .lake/packages --output topo-subset/
lean_split nix Mathlib.CategoryTheory.Functor.Basic --packages-dir .lake/packages --output cat-subset/
```

Each generates a self-contained flake with git-pinned sources. Nix only builds the transitive deps.

## Subcommands

| Command   | Description |
|-----------|-------------|
| `closure` | Transitive import closure (flat list) |
| `dag`     | Import DAG as JSON |
| `decls`   | Content-hash every declaration |
| `extract` | Extract minimal declaration subset by hash |
| `catalog` | Per-module JSON with hash, imports, size |
| `nix`     | Per-file nix derivations + flake.nix (git-pinned sources) |
