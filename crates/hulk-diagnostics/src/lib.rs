pub fn hulk_diagnostics() -> &'static str {
    "hulk-diagnostics"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_diagnostics() {
        assert_eq!(hulk_diagnostics(), "hulk-diagnostics");
    }
}
