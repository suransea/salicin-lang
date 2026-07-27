let array = core.memory.array
let slice = core.memory.slice

/// Constructs a value from the compiler's fixed-size backing array for an
/// array literal. Implementations may preserve the array or build a user type.
pub let array_literal(comptime element: type) = trait {
  let output: type

  let from_array_literal(comptime length: usize)
    (move values: array(element)(length)): output
}

/// Constructs a value from the UTF-8 backing bytes of a string literal.
pub let string_literal = trait {
  let output: type

  let from_string_literal(comptime length: usize)
    (move utf8: array(u8)(length)): output
}

/// The fixed-size array implementation preserves the compiler backing value.
extend(array(t)(l), array_literal(t)) {
  let output = array(t)(l)

  let from_array_literal(comptime length: usize)
    (move values: array(t)(length)): output = builtin()
}

/// A UTF-8 byte array can preserve string-literal backing without conversion.
extend(array(u8)(l), string_literal) {
  let output = array(u8)(l)

  let from_string_literal(comptime length: usize)
    (move utf8: array(u8)(length)): output = builtin()
}

/// A slice literal is a borrow of compiler-owned literal backing storage.
extend(slice(t), array_literal(t)) {
  let output = slice(t)

  let from_array_literal(comptime length: usize)
    (move values: array(t)(length)): output = builtin()
}

/// UTF-8 slices may be selected directly as the result of a string literal.
extend(slice(u8), string_literal) {
  let output = slice(u8)

  let from_string_literal(comptime length: usize)
    (move utf8: array(u8)(length)): output = builtin()
}
