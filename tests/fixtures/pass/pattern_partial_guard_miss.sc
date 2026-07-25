let Option = std.Option

let Payload = struct {
  value: i32,
}

let main(): i32 = {
  let offset = 1
  let choose: (Option(Payload)): core.control.Attempt(Option(Payload))(i32) = {
    Some(payload) if payload.value > 100 -> payload.value + offset
  }
  let attempted = choose(Option.Some(Payload { value: 42 }))
  match attempted
    { Hit(_) -> 0 }
    { Miss(remaining) -> match remaining
      { Some(payload) -> payload.value }
      { None -> 0 }
    }
}

test("pattern_partial_guard_miss.sc") {
  main() == 42
}
