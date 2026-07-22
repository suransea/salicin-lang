// Compiler-recognized and standard effect identities. Source spellings such as
// `with(unsafe)` and `throws(E)` bind to the validated `Unsafe` and `Throws(E)`
// declarations rather than to user declarations with the same spelling, but the
// declarations themselves use the same source-level effect forms as user code.
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
