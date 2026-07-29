let cell(comptime t: type) = struct { value: t }

extend(cell(t))
(requires: t is copyable) {
  let new(copy value: t): cell(t) = { cell { value: value } }
  let duplicate(self: borrow(self))(): t = {
    let first = self.value
    self.value
  }
}

let read_twice(comptime t: type)(cell: borrow(cell(t))): t = requires(t is copyable) {
  cell.duplicate()
}

let main(): i32 = {
  let cell = cell.new(42)
  read_twice(cell)
}

test("constrained_generic_extend.sc") {
  main() == 42
}
