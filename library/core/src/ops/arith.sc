/// Trait backing binary `+`.
pub let Add(Rhs: type) = trait {
  /// Result type produced by addition.
  let Output: type
  /// Adds `rhs` to `self`.
  let add(move self)
    (move rhs: Rhs): Output
}

/// Trait backing binary `-`.
pub let Sub(Rhs: type) = trait {
  /// Result type produced by subtraction.
  let Output: type
  /// Subtracts `rhs` from `self`.
  let sub(move self)
    (move rhs: Rhs): Output
}

/// Trait backing binary `*`.
pub let Mul(Rhs: type) = trait {
  /// Result type produced by multiplication.
  let Output: type
  /// Multiplies `self` by `rhs`.
  let mul(move self)
    (move rhs: Rhs): Output
}

/// Trait backing binary `/`.
pub let Div(Rhs: type) = trait {
  /// Result type produced by division.
  let Output: type
  /// Divides `self` by `rhs`.
  let div(move self)
    (move rhs: Rhs): Output
}

/// Trait backing binary `%`.
pub let Rem(Rhs: type) = trait {
  /// Result type produced by remainder.
  let Output: type
  /// Computes the remainder of `self` divided by `rhs`.
  let rem(move self)
    (move rhs: Rhs): Output
}

/// Trait backing unary numeric negation.
pub let Neg = trait {
  /// Result type produced by negation.
  let Output: type
  /// Negates `self`.
  let neg(move self)(): Output
}
