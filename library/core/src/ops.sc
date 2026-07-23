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

/// Trait backing compound `+=`.
pub let AddAssign(Rhs: type) = trait {
  /// Adds `rhs` into `self` in place.
  let add_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `-=`.
pub let SubAssign(Rhs: type) = trait {
  /// Subtracts `rhs` from `self` in place.
  let sub_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `*=`.
pub let MulAssign(Rhs: type) = trait {
  /// Multiplies `self` by `rhs` in place.
  let mul_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `/=`.
pub let DivAssign(Rhs: type) = trait {
  /// Divides `self` by `rhs` in place.
  let div_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `%=`.
pub let RemAssign(Rhs: type) = trait {
  /// Replaces `self` with its remainder after division by `rhs`.
  let rem_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `&=`.
pub let BitAndAssign(Rhs: type) = trait {
  /// Applies bitwise and with `rhs` in place.
  let bit_and_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `|=`.
pub let BitOrAssign(Rhs: type) = trait {
  /// Applies bitwise or with `rhs` in place.
  let bit_or_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `^=`.
pub let BitXorAssign(Rhs: type) = trait {
  /// Applies bitwise xor with `rhs` in place.
  let bit_xor_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `<<=`.
pub let ShlAssign(Rhs: type) = trait {
  /// Shifts `self` left by `rhs` in place.
  let shl_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing compound `>>=`.
pub let ShrAssign(Rhs: type) = trait {
  /// Shifts `self` right by `rhs` in place.
  let shr_assign(self: borrow(mut)(Self))
    (move rhs: Rhs): ()
}

/// Trait backing equality comparison.
pub let Eq(Rhs: type) = trait {
  /// Returns whether `self` and `rhs` compare equal.
  let eq(self: borrow(Self))
    (rhs: borrow(Rhs)): bool
}

/// Four-way result for partial comparison.
pub let PartialOrdering = enum {
  /// `self` is less than the compared value.
  Less,
  /// The compared values are equivalent for ordering.
  Equal,
  /// `self` is greater than the compared value.
  Greater,
  /// The values cannot be ordered relative to each other.
  Unordered,
}

/// Trait backing partial ordering comparisons.
pub let PartialOrd(Rhs: type) = trait {
  /// Compares `self` with `rhs`, returning a partial ordering result.
  let partial_cmp(self: borrow(Self))
    (rhs: borrow(Rhs)): PartialOrdering
}

/// Trait backing unary numeric negation.
pub let Neg = trait {
  /// Result type produced by negation.
  let Output: type
  /// Negates `self`.
  let neg(move self)(): Output
}

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

pub use core.flow.{Chain, Coalesce}
