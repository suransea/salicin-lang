// Compiler-recognized and standard effect identities. Source spellings such as
// `with(unsafe)` and `throws(E)` bind to the validated `Unsafe` and `Throws(E)`
// declarations rather than to user declarations with the same spelling.
pub let Unsafe = effect
pub let Throws(E: type) = effect

// A standard marker for asynchronous suspension. The first executable async
// lowering will validate handler and future contracts in core as lang items;
// until then `Async` is an ordinary named effect available through `use`.
pub let Async = effect
