let Payload = struct { value: i32 }

let Event = enum {
  Value( value: Payload ),
  Empty,
}

let accept(move payload: Payload): bool = { payload.value == 42 }

let classify(event: Event): i32 = { event match {
  Event.Value( value: payload ) if accept(payload) => 42,
  Event.Value( value: _ ) => 0,
  Event.Empty => 0,
}
}

let main(): i32 = { classify(Event.Value( value: Payload { value: 42 } )) }
