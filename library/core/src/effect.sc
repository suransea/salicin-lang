// Compiler-recognized and standard effect identities. These declarations use
// the same source-level effect forms as user code; only their validated core
// identity gives them language-item behavior.
// `throw(error)` can also target the ordinary `Throws(E).raise(error)` operation
// when a single standard `Throws(E)` custom effect is active.
// Contextual `try { ... }` can materialize that ordinary effect as `Result(E)(T)`.
/// Authority effect required for operations that can violate language safety.
pub let Unsafe = effect {}

/// Standard effect used to raise errors of type `Error`.
pub let Throws(Error: type) = effect {
  /// Raises `error` and does not return normally.
  let raise(move error: Error): Never
}

// A standard effect identity for asynchronous suspension. Full Future/await
// lowering will replace this minimal operation with the validated async
// contracts in the same implementation slice.
/// Standard effect identity for asynchronous suspension.
pub let Async = effect {
  /// Suspends the current asynchronous computation.
  let suspend(): ()
}
