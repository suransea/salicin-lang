// First-order algebra protocols that do not depend on higher-kinded
// constructor kinds. Law documentation lives in the standard library docs; the
// compiler does not attempt to prove associativity or identity laws.
pub let Semigroup(T: type) = trait {
  let combine(move left: T, move right: T): T
}

pub let Monoid(T: type) = trait {
  let empty(): T
  let combine(move left: T, move right: T): T
}
