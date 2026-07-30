let pair(comptime a: type, comptime b: type) = struct { first: a, second: b }

extend(pair(y, x)) {
  let new(move first: y, move second: x): pair(y, x) = { pair { first: first, second: second } }
  let take_first(move self)(): y = { self.first }
}

let main(): i32 = {
  let pair = pair.new(42, true)
  pair.take_first()
}

test("generic_inherent_reordered.sc") {
  std.test.assert(main() == 42)
}
