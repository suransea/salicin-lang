// Compiler-recognized and standard effect identities. These declarations use
// the same source-level effect forms as user code; only their validated core
// identity gives them language-item behavior.
// `throw(error)` can also target the ordinary `Throws(E).raise(error)` operation
// when a single standard `Throws(E)` custom effect is active.
// Contextual `try { ... }` can materialize that ordinary effect as `Result(E)(T)`.
pub let Unsafe = effect {}

pub let Throws(Error: type) = effect {
  let raise(move error: Error): Never
}

// A standard effect identity for asynchronous suspension. Full Future/await
// lowering will replace this minimal operation with the validated async
// contracts in the same implementation slice.
pub let Async = effect {
  let suspend(): ()
}
