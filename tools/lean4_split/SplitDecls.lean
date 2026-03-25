/-
  SplitDecls.lean — Split mathlib into one-file-per-declaration lattice

  Uses the Lean4 kernel Environment API to:
  1. Load an environment (via import)
  2. Extract every declaration and its exact dependencies
  3. Emit one .lean file per declaration
  4. Generate a lakefile with the lattice import structure

  This gives exact dependency tracking (not regex) and enables:
  - Minimal recompilation: change one decl → rebuild only its dependents
  - Parallel builds: independent branches compile in parallel
  - Cherry-pick imports: `import Decls.HVertexOperator` pulls only what's needed

  Usage:
    lake env lean SplitDecls.lean -- Mathlib.Algebra.Vertex.HVertexOperator ./out
-/
import Lean

open Lean System

/-- Collect all constant names referenced in an expression -/
def collectRefs (e : Expr) : NameSet :=
  e.foldConsts {} fun n s => s.insert n

/-- Get all dependencies of a constant (from type + value) -/
def constDeps (env : Environment) (n : Name) : NameSet :=
  match env.find? n with
  | some ci =>
    let fromType := collectRefs ci.type
    let fromVal := match ci with
      | .defnInfo v => collectRefs v.value
      | .thmInfo v  => collectRefs v.value
      | .opaqueInfo v => collectRefs v.value
      | _ => {}
    fromType.union fromVal
  | none => {}

/-- Filter to only names that are in our target set -/
def filterDeps (deps : NameSet) (targets : NameSet) : Array Name :=
  deps.fold (init := #[]) fun acc n =>
    if targets.contains n then acc.push n else acc

/-- Sanitize a Lean name into a valid filename component -/
def nameToFile (n : Name) : String :=
  n.toString.replace "." "/" ++ ".lean"

/-- Sanitize a Lean name into a module name for imports -/
def nameToMod (n : Name) : String :=
  "Decls." ++ n.toString.replace "." ".Decls."
  |>.replace ".Decls.Decls." ".Decls."

/-- Simple module name: just replace dots -/
def nameToSimpleMod (n : Name) : String :=
  let s := n.toString
  "Split." ++ s.replace "." "_"

/-- BFS closure from a root name -/
def bfsClosure (env : Environment) (root : Name) (universe : NameSet) : NameSet := Id.run do
  let mut visited : NameSet := {}
  let mut queue : Array Name := #[root]
  while h : queue.size > 0 do
    let n := queue[0]
    queue := queue.extract 1 queue.size
    if visited.contains n then continue
    if !universe.contains n then continue
    visited := visited.insert n
    let deps := constDeps env n
    for d in deps.toList do
      if universe.contains d && !visited.contains d then
        queue := queue.push d
  return visited

/-- Topological sort of names by dependency -/
def topoSort (env : Environment) (names : Array Name) (universe : NameSet) : Array Name := Id.run do
  let mut sorted : Array Name := #[]
  let mut visited : NameSet := {}
  -- Simple iterative topo sort
  let mut remaining := names
  let mut maxIter := names.size * 2
  while remaining.size > 0 && maxIter > 0 do
    maxIter := maxIter - 1
    let mut nextRemaining : Array Name := #[]
    for n in remaining do
      let deps := filterDeps (constDeps env n) universe
      let allSatisfied := deps.all fun d => visited.contains d || !universe.contains d
      if allSatisfied then
        sorted := sorted.push n
        visited := visited.insert n
      else
        nextRemaining := nextRemaining.push n
    if nextRemaining.size == remaining.size then
      -- Cycle: force one through
      sorted := sorted.push remaining[0]!
      visited := visited.insert remaining[0]!
      nextRemaining := remaining.extract 1 remaining.size
    remaining := nextRemaining
  return sorted

/-- Emit a single .lean file for one declaration -/
def emitDeclFile (env : Environment) (n : Name) (deps : Array Name)
    (outDir : FilePath) : IO Unit := do
  let modName := nameToSimpleMod n
  let relPath := modName.replace "." "/" ++ ".lean"
  let path := outDir / relPath

  -- Ensure parent dir exists
  IO.FS.createDirAll path.parent.getD outDir

  let mut lines : Array String := #[]

  -- Emit imports for dependencies
  for d in deps do
    lines := lines.push s!"import {nameToSimpleMod d}"

  if deps.size > 0 then
    lines := lines.push ""

  -- Emit the declaration info as a comment
  lines := lines.push s!"-- {n} from environment"

  match env.find? n with
  | some (.defnInfo v) =>
    lines := lines.push s!"-- def {n} : {v.type}"
    lines := lines.push s!"-- (definition body omitted — use `lake env lean --run` to extract)"
  | some (.thmInfo v) =>
    lines := lines.push s!"-- theorem {n} : {v.type}"
  | some (.axiomInfo v) =>
    lines := lines.push s!"-- axiom {n} : {v.type}"
  | some (.opaqueInfo v) =>
    lines := lines.push s!"-- opaque {n} : {v.type}"
  | some (.inductInfo v) =>
    lines := lines.push s!"-- inductive {n} : {v.type}"
    lines := lines.push s!"--   ctors: {v.ctors}"
  | some (.ctorInfo v) =>
    lines := lines.push s!"-- constructor {n} : {v.type}"
  | some (.recInfo v) =>
    lines := lines.push s!"-- recursor {n} : {v.type}"
  | some (.quotInfo _) =>
    lines := lines.push s!"-- quot {n}"
  | none =>
    lines := lines.push s!"-- (not found)"

  -- Placeholder: re-export or sorry stub
  lines := lines.push ""
  lines := lines.push s!"-- Stub: this file represents the declaration `{n}`"
  lines := lines.push s!"-- In a full split, the body would be extracted from the environment."

  IO.FS.writeFile path (String.intercalate "\n" lines.toList ++ "\n")

/-- Generate lakefile.toml for the split project -/
def emitLakefile (names : Array Name) (outDir : FilePath) : IO Unit := do
  let mut lines : Array String := #[]
  lines := lines.push "name = \"mathlib-split\""
  lines := lines.push "version = \"0.1.0\""
  lines := lines.push ""
  lines := lines.push "[[require]]"
  lines := lines.push "name = \"mathlib\""
  lines := lines.push "git = \"https://github.com/meta-introspector/mathlib4\""
  lines := lines.push "rev = \"feature/split\""
  lines := lines.push ""
  -- One lean_lib per top-level namespace
  let mut seen : NameSet := {}
  for n in names do
    let mod := nameToSimpleMod n
    let root := mod.splitOn "." |>.head? |>.getD "Split"
    let rootName := root.toName
    if !seen.contains rootName then
      seen := seen.insert rootName
      lines := lines.push "[[lean_lib]]"
      lines := lines.push s!"name = \"{root}\""
      lines := lines.push ""
  IO.FS.writeFile (outDir / "lakefile.toml") (String.intercalate "\n" lines.toList ++ "\n")

/-- Generate a DAG summary as JSON -/
def emitDag (env : Environment) (names : Array Name) (universe : NameSet)
    (outDir : FilePath) : IO Unit := do
  let mut lines : Array String := #[]
  lines := lines.push "{"
  for (i, n) in names.toList.enum do
    let deps := filterDeps (constDeps env n) universe
    let depsStr := deps.map (fun d => s!"\"{d}\"") |>.toList
    let comma := if i + 1 < names.size then "," else ""
    lines := lines.push s!"  \"{n}\": [{String.intercalate ", " depsStr}]{comma}"
  lines := lines.push "}"
  IO.FS.writeFile (outDir / "dag.json") (String.intercalate "\n" lines.toList ++ "\n")

/-- Main: split declarations into a lattice -/
def main (args : List String) : IO UInt32 := do
  let rootMod := args.head? |>.getD "Mathlib.Algebra.Vertex.HVertexOperator"
  let outDir : FilePath := args.get? 1 |>.getD "./split-out"

  IO.println s!"SplitDecls: root={rootMod} out={outDir}"

  -- Import the environment
  let opts := {}
  let env ← importModules #[{ module := rootMod.toName }] opts

  -- Collect all non-internal constants
  let mut allNames : NameSet := {}
  let mut allNamesArr : Array Name := #[]
  for (n, _) in env.constants.map₁.toList do
    -- Skip internal names
    if n.isInternal then continue
    if n.toString.startsWith "_" then continue
    allNames := allNames.insert n
    allNamesArr := allNamesArr.push n

  IO.println s!"  {allNamesArr.size} declarations in environment"

  -- BFS closure from root
  let rootName := rootMod.toName
  let closure := bfsClosure env rootName allNames
  let closureArr := closure.toList.toArray

  IO.println s!"  {closureArr.size} declarations in closure of {rootMod}"

  -- Topological sort
  let sorted := topoSort env closureArr closure

  IO.println s!"  {sorted.size} declarations after topo sort"

  -- Create output directory
  IO.FS.createDirAll outDir

  -- Emit one file per declaration
  let mut count := 0
  for n in sorted do
    let deps := filterDeps (constDeps env n) closure
    emitDeclFile env n deps outDir
    count := count + 1
    if count % 500 == 0 then
      IO.println s!"  ... {count}/{sorted.size} files emitted"

  -- Emit lakefile
  emitLakefile sorted outDir

  -- Emit DAG
  emitDag env sorted closure outDir

  IO.println s!"  ✅ {count} declaration files written to {outDir}"
  IO.println s!"  ✅ dag.json written"
  IO.println s!"  ✅ lakefile.toml written"

  return 0
