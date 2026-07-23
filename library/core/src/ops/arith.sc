/// Trait backing binary `+`.
pub let Add(Rhs: type) = trait {
  /// Result type produced by addition.
  let Output: type
  /// Adds `rhs` to `self`.
  let add(self)
    (rhs: Rhs): Output
}

/// Trait backing binary `-`.
pub let Sub(Rhs: type) = trait {
  /// Result type produced by subtraction.
  let Output: type
  /// Subtracts `rhs` from `self`.
  let sub(self)
    (rhs: Rhs): Output
}

/// Trait backing binary `*`.
pub let Mul(Rhs: type) = trait {
  /// Result type produced by multiplication.
  let Output: type
  /// Multiplies `self` by `rhs`.
  let mul(self)
    (rhs: Rhs): Output
}

/// Trait backing binary `/`.
pub let Div(Rhs: type) = trait {
  /// Result type produced by division.
  let Output: type
  /// Divides `self` by `rhs`.
  let div(self)
    (rhs: Rhs): Output
}

/// Trait backing binary `%`.
pub let Rem(Rhs: type) = trait {
  /// Result type produced by remainder.
  let Output: type
  /// Computes the remainder of `self` divided by `rhs`.
  let rem(self)
    (rhs: Rhs): Output
}

/// Trait backing unary numeric negation.
pub let Neg = trait {
  /// Result type produced by negation.
  let Output: type
  /// Negates `self`.
  let neg(self)(): Output
}
