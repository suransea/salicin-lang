let payload = struct { value: i32 }

extend(payload, copyable) {}

let event = enum {
  value( value: payload ),
  empty,
}

let is_answer(payload: payload): bool = { payload.value == 42 }

let classify(event: event): i32 = { match event
    { event.value( value: payload ) if is_answer(payload) -> payload.value }
    { event.value( value: _ ) -> 0 }
    { event.empty -> 0 }
}

let main(): i32 = { classify(event.value( value: payload { value: 42 } )) }

test("match_guard_copy_binding.sc") {
  std.test.assert(main() == 42)
}
