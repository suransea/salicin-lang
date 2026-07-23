let read('a: region)(value: borrow('a)(i32)): i32 = {
  let alias: borrow('a)(i32) = borrow(value)
  alias
}

let generic_read('a: region, T: type)(cell: borrow('a)(Cell(T))): T
where T: Copy = {
  let alias: borrow('a)(Cell(T)) = borrow(cell)
  alias.value
}

let Cell(T: type) = struct { value: T }

let main(): i32 = {
  let value = 20
  read(value) + generic_read(cell: Cell { value: 22 })
}
