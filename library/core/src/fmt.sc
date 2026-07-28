/// Pure text parsing with a statically selected output and error type.
pub let parse = trait {
  /// Borrowed source type; standard parsing implementations bind this to
  /// `core.string.str`.
  let source: type
  /// Structured error reported for malformed or out-of-range input.
  let error: type

  /// Parses the complete borrowed text. Implementations do not allocate or
  /// accept leading or trailing input unless their concrete contract says so.
  let parse(comptime r: region)
    (value: borrow(r)(source)): core.result(error)(self)
}

/// Effect-polymorphic sink for validated UTF-8 fragments.
pub let text_writer(comptime e: effects) = trait {
  /// Fragment type; conforming text writers bind this to `core.string.str`.
  let text: type
  /// Writes all of `value` or performs one of the declared effects.
  let write(comptime r: region)
    (self: borrow(mut)(self))
    (value: borrow(r)(text)): () with(e)
}

/// Source-backed user-facing formatting.
pub let display = trait {
  /// Fragment type required from the selected writer.
  let text: type
  /// Writes a deterministic display representation without reflection.
  let display(comptime e: effects, comptime w: type)
    (self: borrow(self))
    (writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e, text = text)
}

/// Source-backed diagnostic formatting.
pub let debug = trait {
  /// Fragment type required from the selected writer.
  let text: type
  /// Writes a deterministic diagnostic representation without reflection.
  let debug(comptime e: effects, comptime w: type)
    (self: borrow(self))
    (writer: borrow(mut)(w)): () with(e)
  where w: text_writer(e, text = text)
}
