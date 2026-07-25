let AccessBox(A: access)(T: type) = struct {
  value: borrow(A)(T),
}

let read(value: borrow(i32)): i32 = { value }

let with_access(A: access, T: type)
  (value: borrow(A)(T)): AccessBox(A)(T) = {
  AccessBox(A)(T) { value: value }
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
  main() == 42
}
