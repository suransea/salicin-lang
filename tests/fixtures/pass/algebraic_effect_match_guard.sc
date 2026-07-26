let decide = effect {
  let accept(value: i32): bool
}

let event = enum { value( value: i32 ), empty }

extend event: copyable {}

let accepted(value: i32): bool with(decide) = {
  decide.accept(value)
}

let classify_direct(event: event): i32 with(decide) = {
  match event
    { value( value: value ) if decide.accept(value) -> value }
    { value( value: value ) -> value + 1 }
    { empty -> 0 }
}

let classify_named(event: event): i32 with(decide) = {
  match event
    { value( value: value ) if accepted(value) -> value }
    { value( value: value ) -> value + 1 }
    { empty -> 0 }
}

let main(): i32 = {
  decide.handle accept { (value, resume) -> resume(value == 20) } action {
      classify_direct(event.value( value: 20 )) + classify_named(event.value( value: 21 ))
    }
}

test("algebraic_effect_match_guard.sc") {
  main() == 42
}
