let Option = std.Option

let main(): i32 = {
  let choose: (Option(i32)): core.control.Attempt(Option(i32))(i32) = {
    Some(value) -> value + 1
  }
  let hit = choose(Option.Some(40))
  let miss = choose(Option.None)
  let left = match hit
    { Hit(value) -> value }
    { Miss(_) -> 0 }
  let right = match miss
    { Hit(_) -> 0 }
    { Miss(remaining) -> match remaining
      { Some(_) -> 0 }
      { None -> 1 }
    }
  left + right
}

test("pattern_partial_attempt.sc") {
  main() == 42
}
