// First-order algebra protocols that do not depend on higher-kinded
// constructor sorts. Law documentation lives in the standard library docs; the
// compiler does not attempt to prove associativity or identity laws.
/// Types with an associative binary combination operation.
pub let semigroup = trait {
  /// Combines two values into one value of the same type.
  let combine(left: self, right: self): self
}

/// Semigroups that also provide an identity element.
pub let monoid = trait
where self: semigroup {
  /// Returns the identity element for `combine`.
  let empty(): self
}
