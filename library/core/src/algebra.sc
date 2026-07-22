// First-order algebra protocols that do not depend on higher-kinded
// constructor kinds. Law documentation lives in the standard library docs; the
// compiler does not attempt to prove associativity or identity laws.
pub let Semigroup = trait {
  let combine(move left: Self, move right: Self): Self
}

pub let Monoid = trait
where Self: Semigroup {
  let empty(): Self
}
