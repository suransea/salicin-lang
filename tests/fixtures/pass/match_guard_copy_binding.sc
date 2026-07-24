let Payload = struct { value: i32 }

extend Payload: Copy {}

let Event = enum {
  Value( value: Payload ),
  Empty,
}

let is_answer(payload: Payload): bool = { payload.value == 42 }

let classify(event: Event): i32 = { match event
  { Event.Value( value: payload ) if is_answer(payload) -> payload.value }
  { Event.Value( value: _ ) -> 0 }
  { Event.Empty -> 0 }
}

let main(): i32 = { classify(Event.Value( value: Payload { value: 42 } )) }
