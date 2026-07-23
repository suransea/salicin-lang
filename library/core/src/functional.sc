// Higher-kinded functional protocols. These are normal standard-library
// traits over compile-time type constructors, not prelude items. Law
// documentation belongs in the standard library docs and tests; the compiler
// does not prove these laws.
/// Type constructors whose contained values can be transformed.
pub let Functor = trait(Self: (Value: type): type) {
  /// Applies `transform` to each contained value while preserving structure.
  let map(E: effect, A: type, B: type)
    (move self: Self(A))
    (move transform: (A): B with(E)): Self(B) with(E)
}

/// Functors that can inject plain values and apply contained functions.
pub let Applicative = trait(Self: (Value: type): type)
where Self: Functor {
  /// Lifts a plain value into this type constructor.
  let pure(A: type)
    (move value: A): Self(A)

  /// Applies a contained function to a contained value.
  let apply(E: effect, A: type, B: type)
    (move self: Self((A): B with(E)))
    (move value: Self(A)): Self(B) with(E)
}

/// Applicatives that can sequence computations depending on prior values.
pub let Monad = trait(Self: (Value: type): type)
where Self: Applicative {
  /// Sequences `self` into the next computation.
  let flat_map(E: effect, A: type, B: type)
    (move self: Self(A))
    (move next: (A): Self(B) with(E)): Self(B) with(E)
}
