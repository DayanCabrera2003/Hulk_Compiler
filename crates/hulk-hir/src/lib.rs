pub fn hulk_hir() -> &'static str {
    "hulk-hir"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_hir() {
        assert_eq!(hulk_hir(), "hulk-hir");
    }
}
