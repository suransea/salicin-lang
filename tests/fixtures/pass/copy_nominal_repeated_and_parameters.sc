let pair = struct { left: i32, right: i32 }

extend(pair, copyable) {}

let read_left(pair: pair): i32 = { pair.left }

let read_right(copy pair: pair): i32 = { pair.right }

let main(): i32 = {
  let pair = pair { left: 10, right: 11 }
  let first = pair
  let second = pair
  first.left + second.right + read_left(pair) + read_right(pair)
}

test("copy_nominal_repeated_and_parameters.sc") {
  std.test.assert(main() == 42)
}
