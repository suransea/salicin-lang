let cell(comptime t: type) = struct { value: t }

extend(cell(t))
where t: copyable {
  let new(copy value: t): cell(t) = { cell { value: value } }
  let duplicate(self: borrow(self))(): t = {
    let first = self.value
    self.value
  }
}

let read_twice(comptime t: type)(cell: borrow(cell(t))): t
where t: copyable = { cell.duplicate() }

let main(): i32 = {
  let cell = cell.new(42)
  read_twice(cell)
}

test("constrained_generic_extend.sc") {
  main() == 42
}
