// Test de arquitectura: verifica dependencias entre crates y ciclos
// Requiere cargo_metadata como dev-dependency

use std::collections::{HashMap, HashSet};
use cargo_metadata::MetadataCommand;

/// Define la whitelist de dependencias permitidas entre crates.
/// Fuente de verdad: diagrama de dependencias en PIPELINE.md.
fn allowed_deps() -> HashMap<&'static str, HashSet<&'static str>> {
    use std::iter::FromIterator;
    HashMap::from([
        ("hulk-span", HashSet::new()),
        ("hulk-tokens", HashSet::from_iter(["hulk-span"].into_iter())),
        ("hulk-ast", HashSet::from_iter(["hulk-span"].into_iter())),
        ("hulk-diagnostics", HashSet::from_iter(["hulk-span"].into_iter())),
        ("hulk-lexer", HashSet::from_iter(["hulk-tokens", "hulk-diagnostics"].into_iter())),
        ("hulk-parser", HashSet::from_iter(["hulk-ast", "hulk-tokens", "hulk-diagnostics"].into_iter())),
        ("hulk-semantic", HashSet::from_iter(["hulk-ast", "hulk-diagnostics"].into_iter())),
        ("hulk-types", HashSet::from_iter(["hulk-ast", "hulk-semantic", "hulk-diagnostics"].into_iter())),
        ("hulk-hir", HashSet::from_iter(["hulk-ast", "hulk-semantic", "hulk-types"].into_iter())),
        ("hulk-macros", HashSet::from_iter(["hulk-hir", "hulk-diagnostics"].into_iter())),
        ("hulk-desugar", HashSet::from_iter(["hulk-hir", "hulk-diagnostics"].into_iter())),
        ("hulk-banner", HashSet::from_iter(["hulk-hir", "hulk-diagnostics"].into_iter())),
        ("hulk-codegen", HashSet::from_iter(["hulk-banner", "hulk-diagnostics"].into_iter())),
        ("hulk-driver", HashSet::from_iter(["hulk-lexer", "hulk-parser", "hulk-semantic", "hulk-types", "hulk-hir", "hulk-macros", "hulk-desugar", "hulk-banner", "hulk-codegen"].into_iter())),
        ("hulk-cli", HashSet::from_iter(["hulk-driver"].into_iter())),
    ])
}

/// Detecta ciclos en el grafo de dependencias entre crates locales
fn detect_cycles<'a>(graph: &'a HashMap<&'a str, HashSet<&'a str>>) -> Vec<Vec<&'a str>> {
    fn visit<'a>(
        node: &'a str,
        graph: &'a HashMap<&'a str, HashSet<&'a str>>,
        stack: &mut Vec<&'a str>,
        visited: &mut HashSet<&'a str>,
        rec_stack: &mut HashSet<&'a str>,
        cycles: &mut Vec<Vec<&'a str>>,
    ) {
        if !visited.insert(node) {
            return;
        }
        rec_stack.insert(node);
        stack.push(node);
        if let Some(neigh) = graph.get(node) {
            for &n in neigh {
                if rec_stack.contains(n) {
                    // ciclo encontrado
                    let idx = stack.iter().position(|&x| x == n).unwrap();
                    cycles.push(stack[idx..].to_vec());
                } else {
                    visit(n, graph, stack, visited, rec_stack, cycles);
                }
            }
        }
        rec_stack.remove(node);
        stack.pop();
    }
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut stack = Vec::new();
    let mut cycles = Vec::new();
    for &node in graph.keys() {
        if !visited.contains(node) {
            visit(node, graph, &mut stack, &mut visited, &mut rec_stack, &mut cycles);
        }
    }
    cycles
}

#[test]
fn test_layer_dependencies() {
    let metadata = MetadataCommand::new().exec().expect("cargo metadata failed");
    let workspace_members: HashSet<_> = metadata.workspace_members.iter().map(|id| {
        metadata.packages.iter().find(|p| &p.id == id).unwrap().name.as_str()
    }).collect();
    let allowed = allowed_deps();

    // Verifica que todos los crates estén en la whitelist
    for member in &workspace_members {
        assert!(allowed.contains_key(member), "Crate '{member}' no está en la whitelist de dependencias. Actualiza allowed_deps().");
    }
    for key in allowed.keys() {
        assert!(workspace_members.contains(key), "Crate '{key}' está en la whitelist pero no existe en el workspace.");
    }

    // Construye el grafo de dependencias locales
    let mut graph: HashMap<&str, HashSet<&str>> = HashMap::new();
    for pkg in &metadata.packages {
        let name = pkg.name.as_str();
        if !allowed.contains_key(name) { continue; }
        let mut local_deps = HashSet::new();
        for dep in &pkg.dependencies {
            let dep_name = dep.name.as_str();
            if workspace_members.contains(dep_name) {
                local_deps.insert(dep_name);
            }
        }
        graph.insert(name, local_deps);
    }

    // Verifica dependencias prohibidas
    let mut errors = Vec::new();
    for (krate, deps) in &graph {
        let allowed_set = &allowed[krate];
        for dep in deps {
            if !allowed_set.contains(dep) {
                errors.push(format!("Dependencia prohibida: {krate} → {dep}"));
            }
        }
    }
    if !errors.is_empty() {
        panic!("Violaciones de la regla de capas:\n{}", errors.join("\n"));
    }

    // Verifica ciclos
    let cycles = detect_cycles(&graph);
    if !cycles.is_empty() {
        let msg = cycles.iter().map(|c| format!("Ciclo: {}", c.join(" → "))).collect::<Vec<_>>().join("\n");
        panic!("Ciclos detectados en el grafo de dependencias:\n{msg}");
    }
}
