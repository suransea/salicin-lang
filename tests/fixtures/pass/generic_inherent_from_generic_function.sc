let cell(comptime t: type) = struct { value: t }

extend(comptime t: type) cell(t) {
  let take(move self)(): t = { self.value }
}

let consume(comptime t: type)(move cell: cell(t)): t = { cell.take() }

let main(): i32 = { consume(cell: cell { value: 42 }) }

test("generic_inherent_from_generic_function.sc") {
  main() == 42
}
