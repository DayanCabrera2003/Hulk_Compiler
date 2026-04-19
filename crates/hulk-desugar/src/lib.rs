pub fn hulk_desugar() -> &'static str {
    "hulk-desugar"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_desugar() {
        assert_eq!(hulk_desugar(), "hulk-desugar");
    }
}
