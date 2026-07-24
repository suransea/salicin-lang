let Decide = effect {
  let accept(value: i32): bool
}

let Event = enum { Value( value: i32 ), Empty }

extend Event: Copy {}

let accepted(value: i32): bool with(Decide) = {
  Decide.accept(value)
}

let classify_direct(event: Event): i32 with(Decide) = {
  match event
    { Value( value: value ) if Decide.accept(value) -> value }
    { Value( value: value ) -> value + 1 }
    { Empty -> 0 }
}

let classify_named(event: Event): i32 with(Decide) = {
  match event
    { Value( value: value ) if accepted(value) -> value }
    { Value( value: value ) -> value + 1 }
    { Empty -> 0 }
}

let main(): i32 = {
  Decide.handle(
    accept: { (value, resume) -> resume(value == 20) },
  ) {
    classify_direct(Event.Value( value: 20 )) + classify_named(Event.Value( value: 21 ))
  }
}
