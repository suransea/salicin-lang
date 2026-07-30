let cell(comptime t: type) = struct { value: t }

extend(cell(t)) {
  let take(move self)(): t = { self.value }
  let round_trip(move value: t): t = {
    let cell = cell { value: value }
    cell.take()
  }
}

let main(): i32 = { cell.round_trip(42) }

test("generic_inherent_internal_dispatch.sc") {
  std.test.assert(main() == 42)
}
