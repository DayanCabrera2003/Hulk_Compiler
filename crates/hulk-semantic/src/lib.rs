pub fn hulk_semantic() -> &'static str {
    "hulk-semantic"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_semantic() {
        assert_eq!(hulk_semantic(), "hulk-semantic");
    }
}
