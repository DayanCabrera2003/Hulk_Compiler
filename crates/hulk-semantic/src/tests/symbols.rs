use crate::{SymbolKind, SymbolTable};

use super::common::test_span;

#[test]
fn symbol_table_add_get_and_name_of_work() {
    let mut table = SymbolTable::new();
    let span = test_span();
    let id = table.add("x", SymbolKind::Variable, span.clone());

    let Some(symbol) = table.get(id) else {
        panic!("symbol should exist");
    };
    assert_eq!(symbol.id, id);
    assert_eq!(symbol.name, "x");
    assert_eq!(symbol.kind, SymbolKind::Variable);
    assert_eq!(symbol.span, span);
    assert_eq!(table.name_of(id), Some("x"));
}
