let option = core.option

let payload = struct {
  value: i32,
}

let main(): i32 = {
  let offset = 1
  let choose: (option(payload)): core.control.attempt(option(payload))(i32) = {
    some(payload) if payload.value > 100 -> payload.value + offset
  }
  let attempted = choose(option.some(payload { value: 42 }))
  match attempted
    { hit(_) -> 0 }
    { miss(remaining) -> match remaining
      { some(payload) -> payload.value }
      { none -> 0 }
    }
}

test("pattern_partial_guard_miss.sc") {
  std.test.assert(main() == 42)
}
