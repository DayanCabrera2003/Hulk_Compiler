pub fn hulk_driver() -> &'static str {
    "hulk-driver"
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_driver() {
        assert_eq!(hulk_driver(), "hulk-driver");
    }
}
