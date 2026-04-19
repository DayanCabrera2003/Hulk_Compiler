pub fn hulk_tokens() -> &'static str {
    "hulk-tokens"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_tokens() {
        assert_eq!(hulk_tokens(), "hulk-tokens");
    }
}
