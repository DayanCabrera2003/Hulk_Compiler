pub fn hulk_types() -> &'static str {
    "hulk-types"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_types() {
        assert_eq!(hulk_types(), "hulk-types");
    }
}
