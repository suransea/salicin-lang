let read(comptime r: region)(value: borrow(r)(i32)): i32 = {
  let alias: borrow(r)(i32) = borrow(value)
  alias
}

let generic_read(comptime r: region, comptime t: type)(cell: borrow(r)(cell(t))): t
= requires(t is copyable) {
  let alias: borrow(r)(cell(t)) = borrow(cell)
  alias.value
}

let cell(comptime t: type) = struct { value: t }

let main(): i32 = {
  let value = 20
  read(value) + generic_read(cell: cell { value: 22 })
}

test("region_scoped_borrow.sc") {
  main() == 42
}
