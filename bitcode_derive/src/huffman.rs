use crate::attribute::PrefixCode;

/// Returns tuples of (code, bit length) in same order as input frequencies.
pub fn huffman(frequencies: &[f64], max_len: u8) -> Vec<PrefixCode> {
    struct Symbol {
        index: usize,
        code: u32,
        len: u8,
    }

    let frequencies = frequencies.iter().map(|f| f.max(0.0)).collect::<Vec<_>>();
    let lengths = packagemerge::package_merge(&frequencies, max_len as u32).unwrap();
    let mut symbols = lengths
        .into_iter()
        .enumerate()
        .map(|(index, len)| Symbol {
            index,
            code: u32::MAX,
            len: len as u8,
        })
        .collect::<Vec<_>>();
    symbols.sort_by_key(|symbol| (symbol.len, symbol.index));
    let mut code: u32 = 0;
    let mut last_len: u8 = 0;
    for (i, symbol) in symbols.iter_mut().enumerate() {
        if i > 0 {
            code = (code + 1) << (symbol.len - last_len);
        }
        symbol.code = code;
        last_len = symbol.len;
    }
    symbols.sort_by_key(|symbol| symbol.index);

    symbols
        .into_iter()
        .map(|symbol| PrefixCode {
            value: symbol.code.reverse_bits() >> (u32::BITS - symbol.len as u32),
            bits: symbol.len as usize,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::huffman;

    #[test]
    fn unconstrained() {
        let symbol_frequencies = vec![('a', 10), ('b', 1), ('c', 15), ('d', 7)];
        let frequencies = symbol_frequencies
            .iter()
            .map(|(_, l)| *l as f64)
            .collect::<Vec<_>>();
        let code_len = huffman(&frequencies, 3);
        assert_eq!(code_len, vec![(0b10, 2), (0b110, 3), (0b0, 1), (0b111, 3)]);
    }

    #[test]
    fn constrained() {
        let symbol_frequencies = vec![('a', 10), ('b', 1), ('c', 15), ('d', 7)];
        let frequencies = symbol_frequencies
            .iter()
            .map(|(_, l)| *l as f64)
            .collect::<Vec<_>>();
        let code_len = huffman(&frequencies, 2);
        assert_eq!(code_len, vec![(0, 2), (1, 2), (2, 2), (3, 2)]);
    }
}
