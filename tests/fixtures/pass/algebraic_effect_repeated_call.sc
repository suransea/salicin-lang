let read = effect {
  let read(value: i32): i32
}

let once: with(read)(value: i32): i32 = {
  read.read(value)
}

let main(): i32 = {
  read.handle read { (value, resume) -> resume(value) } action {
      once(19) + once(23)
    }
}

test("algebraic_effect_repeated_call.sc") {
  main() == 42
}
