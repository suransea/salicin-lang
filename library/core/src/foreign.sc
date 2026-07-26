/// Calling conventions accepted by foreign declarations.
pub let abi = sort {
  c
}

/// Declares a function whose implementation is supplied by a foreign ABI.
pub let foreign(comptime abi: abi): never = builtin()
