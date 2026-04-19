pub fn hulk_codegen() -> &'static str {
    "hulk-codegen"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_codegen() {
        assert_eq!(hulk_codegen(), "hulk-codegen");
    }
}
