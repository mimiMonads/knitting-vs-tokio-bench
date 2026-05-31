pub const BYTE_FILL_VALUES: &[u8] = &[0xAB, 0xBC, 0xCD, 0xDE];

pub fn make_byte_payloads(bytes: usize) -> Vec<Vec<u8>> {
    BYTE_FILL_VALUES.iter().map(|&fill| vec![fill; bytes]).collect()
}

pub fn power_of_two_sizes(min: usize, max: usize) -> Vec<usize> {
    let mut sizes = Vec::new();
    let mut n = min;
    while n <= max {
        sizes.push(n);
        n *= 2;
    }
    sizes
}
