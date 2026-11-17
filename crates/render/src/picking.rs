//! Codifica ID di picking (R32Uint): 0 = nessun elemento, indice locale + 1 altrimenti.

/// Codifica l'indice locale della mesh nel valore scritto nel target di picking.
pub fn encode_pick_id(index: u32) -> u32 {
    index.checked_add(1).expect("indice di picking fuori range")
}

/// Decodifica un valore letto dal target di picking (`None` = sfondo).
pub fn decode_pick_id(value: u32) -> Option<u32> {
    value.checked_sub(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_id_encoding_roundtrip() {
        assert_eq!(decode_pick_id(0), None);
        for i in [0u32, 1, 42, u32::MAX - 1] {
            assert_eq!(decode_pick_id(encode_pick_id(i)), Some(i));
        }
    }
}
