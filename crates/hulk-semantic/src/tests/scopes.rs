use crate::{Resolver, SymbolKind};

use super::common::test_span;

#[test]
fn resolver_push_and_pop_scopes() {
    let mut resolver = Resolver::new();
    let root_len = resolver.scopes.len();

    resolver.push_scope();
    assert_eq!(resolver.scopes.len(), root_len + 1);
    assert!(resolver.pop_scope().is_some());
    assert_eq!(resolver.scopes.len(), root_len);
    assert!(resolver.pop_scope().is_none());
}

#[test]
fn resolver_lookup_finds_local_and_outer_bindings() {
    let mut resolver = Resolver::new();
    let span = test_span();

    let global = resolver.define("x", SymbolKind::Variable, span.clone());
    resolver.push_scope();
    let local = resolver.define("y", SymbolKind::Variable, span);

    assert_eq!(resolver.lookup("x"), Some(global));
    assert_eq!(resolver.lookup("y"), Some(local));
    assert_eq!(resolver.lookup("missing"), None);
}

#[test]
fn resolver_registers_builtins_in_global_scope() {
    let resolver = Resolver::new();

    for name in [
        "print", "sqrt", "sin", "cos", "exp", "log", "rand", "range", "PI", "E",
    ] {
        let Some(id) = resolver.lookup(name) else {
            panic!("builtin should resolve: {name}");
        };
        assert_eq!(resolver.table().name_of(id), Some(name));
    }
}
