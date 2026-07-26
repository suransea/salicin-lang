let payload = struct { value: i32 }

let event = enum {
  value( value: payload ),
  empty,
}

let accept(move payload: payload): bool = { payload.value == 42 }

let classify(event: event): i32 = { match event
    { event.value( value: payload ) if accept(payload) -> 42 }
    { event.value( value: _ ) -> 0 }
    { event.empty -> 0 }
}

let main(): i32 = { classify(event.value( value: payload { value: 42 } )) }
