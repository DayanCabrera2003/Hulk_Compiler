pub fn hulk_banner() -> &'static str {
    "hulk-banner"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_banner() {
        assert_eq!(hulk_banner(), "hulk-banner");
    }
}
