/// Trait backing binary `+`.
pub let add_operator(comptime rhs: type) = trait {
  /// Result type produced by addition.
  let output: type
  /// Adds `rhs` to `self`.
  let add(self)
    (rhs: rhs): output
}

/// Trait backing binary `-`.
pub let sub_operator(comptime rhs: type) = trait {
  /// Result type produced by subtraction.
  let output: type
  /// Subtracts `rhs` from `self`.
  let sub(self)
    (rhs: rhs): output
}

/// Trait backing binary `*`.
pub let mul_operator(comptime rhs: type) = trait {
  /// Result type produced by multiplication.
  let output: type
  /// Multiplies `self` by `rhs`.
  let mul(self)
    (rhs: rhs): output
}

/// Trait backing binary `/`.
pub let div_operator(comptime rhs: type) = trait {
  /// Result type produced by division.
  let output: type
  /// Divides `self` by `rhs`.
  let div(self)
    (rhs: rhs): output
}

/// Trait backing binary `%`.
pub let rem_operator(comptime rhs: type) = trait {
  /// Result type produced by remainder.
  let output: type
  /// Computes the remainder of `self` divided by `rhs`.
  let rem(self)
    (rhs: rhs): output
}

/// Trait backing unary numeric negation.
pub let neg_operator = trait {
  /// Result type produced by negation.
  let output: type
  /// Negates `self`.
  let neg(self)(): output
}
