let view(comptime t: type)(comptime a: access)(comptime r: region): type = borrow(a)(r)(t)

let lend = trait {
  let item(comptime a: access)(comptime r: region): type

  let view(comptime a: access, comptime r: region)
    (self: borrow(a)(r)(self))(): item(a)(r)
}

let cell = struct { value: i32 }

extend cell: lend {
  let item = view(i32)

  let view(comptime a: access, comptime r: region)
    (self: borrow(a)(r)(self))(): borrow(a)(r)(i32) = {
    borrow(a)(self.value)
  }
}

let read(value: borrow(i32)): i32 = { value }

let main(): i32 = {
  let mut cell = cell { value: 40 }
  let before = do {
    let value = cell.view(shared)()
    read(value)
  }
  do {
    let value = cell.view(mut)()
    value = before + 2
  }
  let final_value = cell.view(shared)()
  read(final_value)
}

test("gat_borrow_family.sc") {
  main() == 42
}
