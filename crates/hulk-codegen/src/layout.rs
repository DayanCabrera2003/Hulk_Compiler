use std::collections::HashMap;

use hulk_banner::{BannerFunction, BannerProgram};

/// Static layout computed from a `BannerProgram` before any LLVM code is emitted.
///
/// Determines the stable field ordering and global vtable method ordering for
/// every user-defined type. This ordering must be established before emitting
/// any type descriptor or vtable global so that all cross-references are consistent.
pub struct ProgramLayout {
    /// Ordered field names per type (excluding the implicit vtable pointer at offset 0).
    pub fields: HashMap<String, Vec<String>>,
    /// Ordered vtable method names per type (method resolution order, no `__init__`).
    pub vtable_methods: HashMap<String, Vec<String>>,
    /// Global method index: maps each method name to its position in the vtable array.
    ///
    /// The vtable for every type has exactly `all_methods.len()` slots. If a type does
    /// not implement a method, its slot holds a null pointer.
    pub method_index: HashMap<String, usize>,
    /// Canonical global method list (index → name), lexicographically sorted.
    /// Available for inspection and debugging; not read by the codegen directly.
    #[allow(dead_code)]
    pub all_methods: Vec<String>,
}

impl ProgramLayout {
    /// Compute the complete layout from the BANNER program.
    #[must_use]
    pub fn compute(program: &BannerProgram) -> Self {
        let fields = collect_fields(program);
        let all_methods = collect_all_methods(program);
        let method_index = build_method_index(&all_methods);
        let vtable_methods = build_vtable_methods(program, &all_methods);

        Self {
            fields,
            vtable_methods,
            method_index,
            all_methods,
        }
    }

    /// Returns the vtable slot index for a given method name.
    #[must_use]
    pub fn vtable_index(&self, method: &str) -> Option<usize> {
        self.method_index.get(method).copied()
    }

    /// Returns the struct field index (GEP index) for a named field.
    ///
    /// Index 0 is always the vtable pointer, so actual fields start at 1.
    #[must_use]
    pub fn field_index(&self, type_name: &str, field: &str) -> Option<u32> {
        let fields = self.fields.get(type_name)?;
        let pos = fields.iter().position(|f| f == field)?;
        // Field 0 in the struct is the vtable pointer; user fields start at 1.
        Some((pos as u32) + 1)
    }
}

fn collect_fields(program: &BannerProgram) -> HashMap<String, Vec<String>> {
    program
        .types
        .iter()
        .map(|td| (td.name.clone(), td.fields.clone()))
        .collect()
}

/// Collect every non-`__init__` method name across all types.
fn collect_all_methods(program: &BannerProgram) -> Vec<String> {
    let mut names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for td in &program.types {
        for method in vtable_methods_of(td) {
            names.insert(
                method
                    .name
                    .split('.')
                    .next_back()
                    .unwrap_or(&method.name)
                    .to_string(),
            );
        }
    }
    names.into_iter().collect()
}

fn build_method_index(all_methods: &[String]) -> HashMap<String, usize> {
    all_methods
        .iter()
        .enumerate()
        .map(|(i, name)| (name.clone(), i))
        .collect()
}

/// For each type, build the ordered list of method names that appear in its vtable.
///
/// The vtable for type T has one slot per entry in `all_methods`. If T (or any
/// ancestor) implements the method, the slot is non-null during codegen.
fn build_vtable_methods(
    program: &BannerProgram,
    all_methods: &[String],
) -> HashMap<String, Vec<String>> {
    let parent_map: HashMap<&str, Option<&str>> = program
        .types
        .iter()
        .map(|td| (td.name.as_str(), td.parent.as_deref()))
        .collect();

    // Build a set of directly-implemented method names per type.
    let direct_methods: HashMap<&str, std::collections::HashSet<String>> = program
        .types
        .iter()
        .map(|td| {
            let methods: std::collections::HashSet<String> = vtable_methods_of(td)
                .map(|m| m.name.split('.').next_back().unwrap_or(&m.name).to_string())
                .collect();
            (td.name.as_str(), methods)
        })
        .collect();

    program
        .types
        .iter()
        .map(|td| {
            let vtable: Vec<String> = all_methods
                .iter()
                .map(|m| resolve_method_impl(&td.name, m, &parent_map, &direct_methods))
                .collect();
            (td.name.clone(), vtable)
        })
        .collect()
}

/// Returns the fully-qualified function name implementing `method` for `type_name`,
/// walking up the inheritance chain until a definition is found.
fn resolve_method_impl<'a>(
    type_name: &'a str,
    method: &str,
    parent_map: &HashMap<&'a str, Option<&'a str>>,
    direct_methods: &HashMap<&'a str, std::collections::HashSet<String>>,
) -> String {
    let mut current: Option<&str> = Some(type_name);
    while let Some(ty) = current {
        if direct_methods.get(ty).is_some_and(|s| s.contains(method)) {
            return format!("{ty}.{method}");
        }
        current = parent_map.get(ty).and_then(|p| *p);
    }
    // No implementation found — the vtable slot will be null in codegen.
    String::new()
}

/// Iterator over the non-`__init__` methods of a type descriptor.
fn vtable_methods_of(td: &hulk_banner::TypeDescriptor) -> impl Iterator<Item = &BannerFunction> {
    td.methods.iter().filter(|m| !m.name.ends_with(".__init__"))
}
