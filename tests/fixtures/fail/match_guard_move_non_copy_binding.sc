let Payload = struct(value: i32)

let Event = enum {
  Value(Payload),
  Empty,
}

let accept(move payload: Payload): bool = { payload.value == 42 }

let classify(event: Event): i32 = { event match {
  Event.Value(payload) if accept(payload) => 42,
  Event.Value(_) => 0,
  Event.Empty => 0,
}
}

let main(): i32 = { classify(Event.Value(Payload(42))) }
