pub fn hulk_cli() -> &'static str {
    "hulk-cli"
}

fn main() {
    println!("{}", hulk_cli());
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_hulk_cli() {
        assert_eq!(hulk_cli(), "hulk-cli");
    }
}
