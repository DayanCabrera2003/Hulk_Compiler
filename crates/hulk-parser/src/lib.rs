pub fn hulk_parser() -> &'static str {
    "hulk-parser"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_parser() {
        assert_eq!(hulk_parser(), "hulk-parser");
    }
}
