let Payload = struct(value: i32)

extend Payload: Copy {}

let Event = enum {
  Value(Payload),
  Empty,
}

let is_answer(payload: Payload): bool = { payload.value == 42 }

let classify(event: Event): i32 = { event match {
  Event.Value(payload) if is_answer(payload) => payload.value,
  Event.Value(_) => 0,
  Event.Empty => 0,
}
}

let main(): i32 = { classify(Event.Value(Payload(42))) }
