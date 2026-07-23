// First-order algebra protocols that do not depend on higher-kinded
// constructor kinds. Law documentation lives in the standard library docs; the
// compiler does not attempt to prove associativity or identity laws.
/// Types with an associative binary combination operation.
pub let Semigroup = trait {
  /// Combines two values into one value of the same type.
  let combine(move left: Self, move right: Self): Self
}

/// Semigroups that also provide an identity element.
pub let Monoid = trait
where Self: Semigroup {
  /// Returns the identity element for `combine`.
  let empty(): Self
}
