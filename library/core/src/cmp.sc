/// Trait backing eq comparison.
pub let eq(comptime rhs: type) = trait {
  /// Returns whether `self` and `rhs` compare equal.
  let eq(self: borrow(self))
    (rhs: borrow(rhs)): bool
}

/// Four-way result for partial comparison.
pub let partial_ordering = enum {
  /// `self` is less than the compared value.
  less,
  /// The compared values are equivalent for ordering.
  equal,
  /// `self` is greater than the compared value.
  greater,
  /// The values cannot be ordered relative to each other.
  unordered,
}

/// Trait backing partial ordering comparisons.
pub let partial_ord(comptime rhs: type) = trait {
  /// Compares `self` with `rhs`, returning a partial ordering result.
  let partial_cmp(self: borrow(self))
    (rhs: borrow(rhs)): partial_ordering
}
