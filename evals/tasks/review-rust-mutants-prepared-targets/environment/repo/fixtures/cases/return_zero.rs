pub fn increment(_value: u32) -> u32 {
    0
}

#[cfg(test)]
mod tests {
    #[test]
    fn increment_moves_to_the_next_value() {
        assert_eq!(super::increment(4), 5);
    }
}
