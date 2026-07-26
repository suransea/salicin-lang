// Compile-time transformations over runtime parameter schemas and passing modes.
/// Changes a runtime parameter schema to copy its argument.
pub let copy(comptime p: parameters): parameters = builtin()

/// Changes a runtime parameter schema to move its argument.
pub let move(comptime p: parameters): parameters = builtin()

/// Changes a parameter schema to require and erase a compile-time argument.
pub let comptime(comptime p: parameters): parameters = builtin()
