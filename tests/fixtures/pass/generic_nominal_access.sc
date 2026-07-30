let access_box(comptime a: access)(comptime t: type) = struct {
  value: borrow(a)(t),
}

let read(value: borrow(i32)): i32 = { value }

let with_access(comptime a: access, comptime t: type)
  (value: borrow(a)(t)): access_box(a)(t) = {
  access_box(a)(t) { value: value }
}

let main(): i32 = {
  let mut value = 40
  do {
    let cell = with_access(mut, i32)(borrow(mut)(value))
    let target = cell.value
    target = 42
  }
  let cell = with_access(shared, i32)(borrow(value))
  read(cell.value)
}

test("generic_nominal_access.sc") {
  std.test.assert(main() == 42)
}
