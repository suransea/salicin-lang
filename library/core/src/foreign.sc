/// Calling conventions accepted by foreign declarations.
pub let abi = sort(1) {
  c
}

/// Declares a function whose implementation is supplied by a foreign ABI.
pub let foreign(comptime abi: abi): never = builtin()

/// Declares a foreign function with an explicit linker symbol.
pub let foreign(comptime abi: abi, comptime symbol: string): never = builtin()
