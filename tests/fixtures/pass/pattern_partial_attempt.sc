let option = core.option

let main(): i32 = {
  let choose: (option(i32)): core.control.attempt(option(i32))(i32) = {
    some(value) -> value + 1
  }
  let hit = choose(option.some(40))
  let miss = choose(option.none)
  let left = match hit
    { hit(value) -> value }
    { miss(_) -> 0 }
  let right = match miss
    { hit(_) -> 0 }
    { miss(remaining) -> match remaining
      { some(_) -> 0 }
      { none -> 1 }
    }
  left + right
}

test("pattern_partial_attempt.sc") {
  std.test.assert(main() == 42)
}
