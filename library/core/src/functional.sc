// Higher-kinded functional protocols. These are normal standard-library
// traits over compile-time type constructors, not prelude items. Law
// documentation belongs in the standard library docs and tests; the compiler
// does not prove these laws.
/// Type constructors whose contained values can be transformed.
pub let functor = trait(comptime self: (comptime value: type): type) {
  /// Applies `transform` to each contained value while preserving structure.
  let map(comptime e: effects, comptime a: type, comptime b: type)
    (self: self(a))
    (transform: (a): b with(e)): self(b) with(e)
}

/// Functors that can inject plain values and apply contained functions.
pub let applicative = trait(comptime self: (comptime value: type): type)
where self: functor {
  /// Lifts a plain value into this type constructor.
  let pure(comptime a: type)
    (value: a): self(a)

  /// Applies a contained function to a contained value.
  let apply(comptime e: effects, comptime a: type, comptime b: type)
    (self: self((a): b with(e)))
    (value: self(a)): self(b) with(e)
}

/// Applicatives that can sequence computations depending on prior values.
pub let monad = trait(comptime self: (comptime value: type): type)
where self: applicative {
  /// Sequences `self` into the next computation.
  let flat_map(comptime e: effects, comptime a: type, comptime b: type)
    (self: self(a))
    (next: (a): self(b) with(e)): self(b) with(e)
}
