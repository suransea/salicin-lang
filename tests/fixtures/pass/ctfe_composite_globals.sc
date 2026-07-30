let pair = struct {
  left: i32,
  right: i32,
}

let choice = enum {
  pair(pair),
  empty,
}

let make_pair(left: i32, right: i32): pair = {
  pair { left: left, right: right }
}

let choose(value: pair): choice = { choice.pair(value) }

let sum(value: choice): i32 = {
  match value
    { choice.pair(pair(left: left, right: right)) -> left + right }
    { choice.empty -> 0 }
}

let tuple_global: (i32, bool) = (40, true)
let array_global: array(i32)(2) = [tuple_global.0, 2]
let pair_global: pair = make_pair(array_global[0], array_global[1])
let choice_global: choice = choose(pair_global)
let answer: i32 = sum(choice_global)

let main(): i32 = {
  if tuple_global.1 && answer == 42 {
    42
  } else {
    0
  }
}

test("ctfe_composite_globals.sc") {
  std.test.assert(main() == 42)
}
