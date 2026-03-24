//! lean_split — Lean4 import tracer + declaration splitter (Rust).
//!
//! Replaces: trace_imports.py, trace_dag.py, split_decls.py
//!
//! Subcommands:
//!   closure  <root> --packages-dir DIR [--filter PREFIX]
//!   dag      <root> --packages-dir DIR
//!   decls    --modules-file FILE --packages-dir DIR [--output FILE]
//!   extract  --hash HASH --decls FILE
//!   catalog  <root> --packages-dir DIR [--output DIR]  — per-file catalog with deps
//!   nix      <root> --packages-dir DIR [--output DIR]  — per-file nix derivations

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::{env, fs, process};

const PKG_MAP: &[(&str, &str)] = &[
    ("Mathlib", "mathlib"),
    ("Batteries", "batteries"),
    ("Qq", "Qq"),
    ("Aesop", "aesop"),
    ("ProofWidgets", "proofwidgets"),
    ("Cli", "Cli"),
    ("ImportGraph", "importGraph"),
    ("LeanSearchClient", "LeanSearchClient"),
    ("Plausible", "plausible"),
];

fn mod_to_path(module: &str, pkg_dir: &Path) -> Option<PathBuf> {
    let prefix = module.split('.').next()?;
    let dir = PKG_MAP.iter().find(|(p, _)| *p == prefix)?.1;
    let p = pkg_dir.join(dir).join(module.replace('.', "/") + ".lean");
    p.exists().then_some(p)
}

fn get_imports(path: &Path) -> Vec<String> {
    let Ok(src) = fs::read_to_string(path) else { return vec![] };
    src.lines()
        .filter_map(|l| {
            let l = l.trim();
            l.strip_prefix("import ")
                .or_else(|| l.strip_prefix("public import "))
                .map(|m| m.trim().to_string())
        })
        .filter(|m| !m.starts_with("Init") && !m.starts_with("Lean"))
        .collect()
}

// ── closure ────────────────────────────────────────────────────────────

fn cmd_closure(root: &str, pkg_dir: &Path, filter: &str) {
    let mut visited = BTreeSet::new();
    let mut q = VecDeque::from([root.to_string()]);
    while let Some(m) = q.pop_front() {
        if !visited.insert(m.clone()) { continue; }
        if let Some(p) = mod_to_path(&m, pkg_dir) {
            for imp in get_imports(&p) {
                if !visited.contains(&imp) { q.push_back(imp); }
            }
        }
    }
    let mut n = 0;
    for m in &visited {
        if filter.is_empty() || m.starts_with(filter) {
            println!("{m}");
            n += 1;
        }
    }
    eprintln!("{n} modules");
}

// ── dag ────────────────────────────────────────────────────────────────

fn cmd_dag(root: &str, pkg_dir: &Path) {
    let mut dag: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut q = VecDeque::from([root.to_string()]);
    while let Some(m) = q.pop_front() {
        if dag.contains_key(&m) { continue; }
        let imps = mod_to_path(&m, pkg_dir).map(|p| get_imports(&p)).unwrap_or_default();
        for i in &imps {
            if !dag.contains_key(i) { q.push_back(i.clone()); }
        }
        dag.insert(m, imps);
    }
    println!("{}", serde_json::to_string_pretty(&dag).unwrap());
    eprintln!("{} modules", dag.len());
}

// ── decls ──────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone)]
struct Decl {
    name: String,
    kind: String,
    module: String,
    hash: String,
    body: String,
    line: usize,
    refs: Vec<String>,
}

fn content_hash(s: &str) -> String {
    let h = blake3::hash(s.as_bytes());
    h.to_hex()[..16].to_string()
}

fn decl_start(line: &str) -> Option<(&str, String)> {
    let mut l = line.trim_start();
    // skip attributes
    if l.starts_with("@[") {
        if let Some(i) = l.find("] ") { l = l[i + 2..].trim_start(); }
        else { return None; }
    }
    for pfx in &["noncomputable ", "unsafe ", "private ", "protected "] {
        l = l.strip_prefix(pfx).unwrap_or(l);
    }
    let kws = [
        "def ", "theorem ", "lemma ", "abbrev ", "structure ", "class ",
        "instance ", "inductive ", "axiom ", "opaque ",
    ];
    for kw in &kws {
        if let Some(rest) = l.strip_prefix(kw) {
            let name: String = rest.chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.' || *c == '\'')
                .collect();
            if !name.is_empty() {
                return Some((kw.trim(), name));
            }
        }
    }
    None
}

fn parse_decls(path: &Path, module: &str) -> Vec<Decl> {
    let Ok(src) = fs::read_to_string(path) else { return vec![] };
    let lines: Vec<&str> = src.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if lines[i].trim_start().starts_with("import ") {
            i += 1;
            continue;
        }
        if let Some((kind, name)) = decl_start(lines[i]) {
            let start = i;
            i += 1;
            while i < lines.len()
                && decl_start(lines[i]).is_none()
                && !lines[i].trim_start().starts_with("import ")
            {
                i += 1;
            }
            let body = lines[start..i].join("\n").trim().to_string();
            let hash = content_hash(&body);
            out.push(Decl {
                name, kind: kind.to_string(), module: module.to_string(),
                hash, body, line: start + 1, refs: vec![],
            });
        } else {
            i += 1;
        }
    }
    out
}

fn resolve_refs(decls: &mut [Decl]) {
    let idx: BTreeMap<String, String> = decls.iter().map(|d| (d.name.clone(), d.hash.clone())).collect();
    for d in decls.iter_mut() {
        let mut refs = BTreeSet::new();
        for (name, hash) in &idx {
            if *hash != d.hash && d.body.contains(name.as_str()) {
                refs.insert(hash.clone());
            }
        }
        d.refs = refs.into_iter().collect();
    }
}

fn cmd_decls(modules_file: &str, pkg_dir: &Path, output: &str) {
    let mods: Vec<String> = fs::read_to_string(modules_file)
        .unwrap()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    let mut all: Vec<Decl> = Vec::new();
    for m in &mods {
        if let Some(p) = mod_to_path(m, pkg_dir) {
            all.extend(parse_decls(&p, m));
        }
    }
    resolve_refs(&mut all);
    let map: BTreeMap<String, Decl> = all.into_iter().map(|d| (d.hash.clone(), d)).collect();
    let json = serde_json::to_string_pretty(&map).unwrap();
    if output == "-" {
        print!("{json}");
    } else {
        fs::write(output, &json).unwrap();
    }
    eprintln!("{} declarations from {} modules", map.len(), mods.len());
}

// ── extract ────────────────────────────────────────────────────────────

fn cmd_extract(root_hash: &str, decls_file: &str) {
    let map: BTreeMap<String, Decl> =
        serde_json::from_str(&fs::read_to_string(decls_file).unwrap()).unwrap();
    let mut visited = BTreeSet::new();
    let mut q = VecDeque::from([root_hash.to_string()]);
    while let Some(h) = q.pop_front() {
        if !visited.insert(h.clone()) { continue; }
        if let Some(d) = map.get(&h) {
            for r in &d.refs {
                if !visited.contains(r) { q.push_back(r.clone()); }
            }
        }
    }
    let mut decls: Vec<&Decl> = visited.iter().filter_map(|h| map.get(h)).collect();
    decls.sort_by(|a, b| (&a.module, a.line).cmp(&(&b.module, b.line)));
    println!("-- Auto-extracted declaration subset");
    println!("-- {} declarations from {} total", decls.len(), map.len());
    println!();
    let modules: BTreeSet<&str> = decls.iter().map(|d| d.module.as_str()).collect();
    for m in &modules { println!("-- From: {m}"); }
    println!();
    for d in &decls {
        println!("-- [{}] {}.{} ({}, line {})", d.hash, d.module, d.name, d.kind, d.line);
        println!("{}", d.body);
        println!();
    }
    eprintln!("{} declarations extracted", decls.len());
}

// ── catalog ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ModuleEntry {
    module: String,
    hash: String,
    imports: Vec<String>,
    decl_count: usize,
    size_bytes: u64,
}

/// Build full closure returning (visited set, imports map)
fn build_closure(root: &str, pkg_dir: &Path) -> (BTreeSet<String>, BTreeMap<String, Vec<String>>) {
    let mut visited = BTreeSet::new();
    let mut imports_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut q = VecDeque::from([root.to_string()]);
    while let Some(m) = q.pop_front() {
        if !visited.insert(m.clone()) { continue; }
        let imps = mod_to_path(&m, pkg_dir).map(|p| get_imports(&p)).unwrap_or_default();
        for i in &imps {
            if !visited.contains(i) { q.push_back(i.clone()); }
        }
        imports_map.insert(m, imps);
    }
    (visited, imports_map)
}

fn cmd_catalog(root: &str, pkg_dir: &Path, out_dir: &str) {
    let (visited, imports_map) = build_closure(root, pkg_dir);
    let mut entries = Vec::new();
    for m in &visited {
        if let Some(p) = mod_to_path(m, pkg_dir) {
            let src = fs::read_to_string(&p).unwrap_or_default();
            let hash = blake3::hash(src.as_bytes()).to_hex()[..16].to_string();
            let decl_count = src.lines().filter(|l| decl_start(l).is_some()).count();
            let size_bytes = fs::metadata(&p).map(|md| md.len()).unwrap_or(0);
            entries.push(ModuleEntry {
                module: m.clone(), hash,
                imports: imports_map.get(m).cloned().unwrap_or_default(),
                decl_count, size_bytes,
            });
        }
    }
    if out_dir == "-" {
        println!("{}", serde_json::to_string_pretty(&entries).unwrap());
    } else {
        fs::create_dir_all(out_dir).unwrap();
        fs::write(
            Path::new(out_dir).join("catalog.json"),
            serde_json::to_string_pretty(&entries).unwrap(),
        ).unwrap();
        for e in &entries {
            fs::write(
                Path::new(out_dir).join(format!("{}.json", e.module)),
                serde_json::to_string_pretty(e).unwrap(),
            ).unwrap();
        }
        eprintln!("{} modules cataloged → {}/", entries.len(), out_dir);
    }
}

// ── nix ────────────────────────────────────────────────────────────────

fn nix_attr(module: &str) -> String { module.replace('.', "_") }

fn cmd_nix(root: &str, pkg_dir: &Path, out_dir: &str) {
    let (visited, imports_map) = build_closure(root, pkg_dir);
    fs::create_dir_all(out_dir).unwrap();

    let mut attrs: Vec<String> = Vec::new();
    for m in &visited {
        if let Some(p) = mod_to_path(m, pkg_dir) {
            let attr = nix_attr(m);
            let imps = imports_map.get(m).cloned().unwrap_or_default();
            let dep_lines: Vec<String> = imps.iter()
                .filter(|i| visited.contains(*i))
                .map(|i| format!("    modules.{}", nix_attr(i)))
                .collect();
            let deps = if dep_lines.is_empty() { "    # leaf".into() } else { dep_lines.join("\n") };
            let nix = format!(
"# {m}\n{{ lean, modules }}:\nlean.mkDerivation {{\n  name = \"{attr}\";\n  src = {src};\n  deps = [\n{deps}\n  ];\n}}\n",
                m = m, attr = attr, src = p.display(), deps = deps,
            );
            fs::write(Path::new(out_dir).join(format!("{attr}.nix")), nix).unwrap();
            attrs.push(attr);
        }
    }
    attrs.sort();

    // default.nix — lazy, only builds what's referenced
    let body: String = attrs.iter()
        .map(|a| format!("    {a} = import ./{a}.nix {{ inherit lean modules; }};"))
        .collect::<Vec<_>>().join("\n");
    fs::write(Path::new(out_dir).join("default.nix"), format!(
"# {root} — {n} modules, pick what you need\n{{ lean }}:\nlet modules = rec {{\n{body}\n}};\nin modules\n",
        root = root, n = attrs.len(), body = body,
    )).unwrap();

    // select.nix — user picks by attr name
    fs::write(Path::new(out_dir).join("select.nix"), format!(
"# nix-build select.nix --arg wanted '[\"Mathlib_Algebra_Vertex_HVertexOperator\"]'\n{{ lean, wanted ? [\"{default}\"] }}:\nlet all = import ./default.nix {{ inherit lean; }};\nin builtins.map (n: all.${{n}}) wanted\n",
        default = nix_attr(root),
    )).unwrap();

    eprintln!("{} module derivations → {}/", attrs.len(), out_dir);
}

// ── main ───────────────────────────────────────────────────────────────

fn arg_val(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned())
}

fn usage() -> ! {
    eprintln!("lean_split — Lean4 import tracer + declaration splitter");
    eprintln!();
    eprintln!("  lean_split closure <root> --packages-dir DIR [--filter PREFIX]");
    eprintln!("  lean_split dag     <root> --packages-dir DIR");
    eprintln!("  lean_split decls   --modules-file FILE --packages-dir DIR [--output FILE]");
    eprintln!("  lean_split extract --hash HASH --decls FILE");
    eprintln!("  lean_split catalog <root> --packages-dir DIR [--output DIR]");
    eprintln!("  lean_split nix     <root> --packages-dir DIR [--output DIR]");
    process::exit(1);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 { usage(); }
    let pkg = |a: &[String]| PathBuf::from(arg_val(a, "--packages-dir").unwrap_or(".lake/packages".into()));
    match args[1].as_str() {
        "closure" => {
            let root = args.get(2).unwrap_or_else(|| { eprintln!("missing root module"); process::exit(1); });
            cmd_closure(root, &pkg(&args), &arg_val(&args, "--filter").unwrap_or_default());
        }
        "dag" => {
            let root = args.get(2).unwrap_or_else(|| { eprintln!("missing root module"); process::exit(1); });
            cmd_dag(root, &pkg(&args));
        }
        "decls" => {
            let mf = arg_val(&args, "--modules-file").unwrap_or_else(|| { eprintln!("missing --modules-file"); process::exit(1); });
            let out = arg_val(&args, "--output").unwrap_or("-".into());
            cmd_decls(&mf, &pkg(&args), &out);
        }
        "extract" => {
            let hash = arg_val(&args, "--hash").unwrap_or_else(|| { eprintln!("missing --hash"); process::exit(1); });
            let df = arg_val(&args, "--decls").unwrap_or_else(|| { eprintln!("missing --decls"); process::exit(1); });
            cmd_extract(&hash, &df);
        }
        "catalog" => {
            let root = args.get(2).unwrap_or_else(|| { eprintln!("missing root module"); process::exit(1); });
            let out = arg_val(&args, "--output").unwrap_or("-".into());
            cmd_catalog(root, &pkg(&args), &out);
        }
        "nix" => {
            let root = args.get(2).unwrap_or_else(|| { eprintln!("missing root module"); process::exit(1); });
            let out = arg_val(&args, "--output").unwrap_or("nix-modules".into());
            cmd_nix(root, &pkg(&args), &out);
        }
        _ => usage(),
    }
}
