pub fn hulk_lexer() -> &'static str {
    "hulk-lexer"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_lexer() {
        assert_eq!(hulk_lexer(), "hulk-lexer");
    }
}
