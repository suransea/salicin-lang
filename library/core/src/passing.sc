// Compile-time transformations over runtime parameter schemas and passing modes.
/// Changes a runtime parameter schema to copy its argument.
pub let copy(P: parameters): parameters = builtin()

/// Changes a runtime parameter schema to move its argument.
pub let move(P: parameters): parameters = builtin()
