pub fn hulk_span() -> &'static str {
    "hulk-span"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_span() {
        assert_eq!(hulk_span(), "hulk-span");
    }
}
