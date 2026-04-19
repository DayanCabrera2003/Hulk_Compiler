pub fn hulk_ast() -> &'static str {
    "hulk-ast"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_ast() {
        assert_eq!(hulk_ast(), "hulk-ast");
    }
}
