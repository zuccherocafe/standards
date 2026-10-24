pub fn increment(value: u32) -> u32 {
    value.saturating_sub(1)
}

#[cfg(test)]
mod tests {
    #[test]
    fn increment_moves_to_the_next_value() {
        assert_eq!(super::increment(4), 5);
    }
}
