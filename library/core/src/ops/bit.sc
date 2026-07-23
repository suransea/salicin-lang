/// Trait backing unary logical or bitwise not.
pub let Not = trait {
  /// Result type produced by not.
  let Output: type
  /// Inverts `self`.
  let not(move self)(): Output
}

/// Trait backing binary `&`.
pub let BitAnd(Rhs: type) = trait {
  /// Result type produced by bitwise and.
  let Output: type
  /// Computes bitwise and with `rhs`.
  let bit_and(move self)
    (move rhs: Rhs): Output
}

/// Trait backing binary `|`.
pub let BitOr(Rhs: type) = trait {
  /// Result type produced by bitwise or.
  let Output: type
  /// Computes bitwise or with `rhs`.
  let bit_or(move self)
    (move rhs: Rhs): Output
}

/// Trait backing binary `^`.
pub let BitXor(Rhs: type) = trait {
  /// Result type produced by bitwise xor.
  let Output: type
  /// Computes bitwise xor with `rhs`.
  let bit_xor(move self)
    (move rhs: Rhs): Output
}

/// Trait backing binary `<<`.
pub let Shl(Rhs: type) = trait {
  /// Result type produced by left shift.
  let Output: type
  /// Shifts `self` left by `rhs`.
  let shl(move self)
    (move rhs: Rhs): Output
}

/// Trait backing binary `>>`.
pub let Shr(Rhs: type) = trait {
  /// Result type produced by right shift.
  let Output: type
  /// Shifts `self` right by `rhs`.
  let shr(move self)
    (move rhs: Rhs): Output
}
