// Compiler-recognized access identities used by parameter passing and borrow
// types. The surface syntax still uses `borrow` and `borrow(mut)`, while the
// type checker resolves the underlying compile-time access values here.
pub let Shared = access
pub let Mutable = access
