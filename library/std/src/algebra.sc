/// Values with an associative combination operation.
pub let semigroup = trait {
  let combine(move left: self, move right: self): self
}

/// A semigroup with an identity value.
pub let monoid = trait
where self: semigroup {
  let empty(): self
}
