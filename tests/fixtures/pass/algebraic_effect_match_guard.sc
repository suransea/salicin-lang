let decide = effect {
  let accept(value: i32): bool
}

let event = enum { value( value: i32 ), empty }

extend(event, copyable) {}

let accepted: with(decide)(value: i32): bool = {
  decide.accept(value)
}

let classify_direct: with(decide)(event: event): i32 = {
  match event
    { value( value: value ) if decide.accept(value) -> value }
    { value( value: value ) -> value + 1 }
    { empty -> 0 }
}

let classify_named: with(decide)(event: event): i32 = {
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
  std.test.assert(main() == 42)
}
