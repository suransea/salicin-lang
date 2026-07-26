/// Trait backing unary logical or bitwise not.
pub let not = trait {
  /// Result type produced by not.
  let output: type
  /// Inverts `self`.
  let not(self)(): output
}

/// Trait backing binary `&`.
pub let bit_and(comptime rhs: type) = trait {
  /// Result type produced by bitwise and.
  let output: type
  /// Computes bitwise and with `rhs`.
  let bit_and(self)
    (rhs: rhs): output
}

/// Trait backing binary `|`.
pub let bit_or(comptime rhs: type) = trait {
  /// Result type produced by bitwise or.
  let output: type
  /// Computes bitwise or with `rhs`.
  let bit_or(self)
    (rhs: rhs): output
}

/// Trait backing binary `^`.
pub let bit_xor(comptime rhs: type) = trait {
  /// Result type produced by bitwise xor.
  let output: type
  /// Computes bitwise xor with `rhs`.
  let bit_xor(self)
    (rhs: rhs): output
}

/// Trait backing binary `<<`.
pub let shl(comptime rhs: type) = trait {
  /// Result type produced by left shift.
  let output: type
  /// Shifts `self` left by `rhs`.
  let shl(self)
    (rhs: rhs): output
}

/// Trait backing binary `>>`.
pub let shr(comptime rhs: type) = trait {
  /// Result type produced by right shift.
  let output: type
  /// Shifts `self` right by `rhs`.
  let shr(self)
    (rhs: rhs): output
}
