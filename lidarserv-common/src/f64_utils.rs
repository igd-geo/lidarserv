/// Returns the next smaller representable f64 number.
/// todo: use f64::next_down() once it is stable
/// source: copied the nightly impl of f64::next_down().
pub fn f64_next_down(me: f64) -> f64 {
    // We must use strictly integer arithmetic to prevent denormals from
    // flushing to zero after an arithmetic operation on some platforms.
    const NEG_TINY_BITS: u64 = 0x8000_0000_0000_0001; // Smallest (in magnitude) negative f64.
    const CLEAR_SIGN_MASK: u64 = 0x7fff_ffff_ffff_ffff;

    let bits = me.to_bits();
    if me.is_nan() || bits == f64::NEG_INFINITY.to_bits() {
        return me;
    }

    let abs = bits & CLEAR_SIGN_MASK;
    let next_bits = if abs == 0 {
        NEG_TINY_BITS
    } else if bits == abs {
        bits - 1
    } else {
        bits + 1
    };
    f64::from_bits(next_bits)
}

/// Returns the next larger representable f64 number.
/// todo: use f64::next_up() once it is stable
/// source: copied the nightly impl of f64::next_up().
pub fn f64_next_up(me: f64) -> f64 {
    // We must use strictly integer arithmetic to prevent denormals from
    // flushing to zero after an arithmetic operation on some platforms.
    const TINY_BITS: u64 = 0x1; // Smallest positive f64.
    const CLEAR_SIGN_MASK: u64 = 0x7fff_ffff_ffff_ffff;

    let bits = me.to_bits();
    if me.is_nan() || bits == f64::INFINITY.to_bits() {
        return me;
    }

    let abs = bits & CLEAR_SIGN_MASK;
    let next_bits = if abs == 0 {
        TINY_BITS
    } else if bits == abs {
        bits + 1
    } else {
        bits - 1
    };
    f64::from_bits(next_bits)
}
