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
