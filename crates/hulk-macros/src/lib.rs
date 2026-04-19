pub fn hulk_macros() -> &'static str {
    "hulk-macros"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_macros() {
        assert_eq!(hulk_macros(), "hulk-macros");
    }
}
