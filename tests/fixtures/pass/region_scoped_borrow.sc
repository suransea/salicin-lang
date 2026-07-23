let read(R: region)(value: borrow(R)(i32)): i32 = {
  let alias: borrow(R)(i32) = borrow(value)
  alias
}

let generic_read(R: region, T: type)(cell: borrow(R)(Cell(T))): T
where T: Copy = {
  let alias: borrow(R)(Cell(T)) = borrow(cell)
  alias.value
}

let Cell(T: type) = struct { value: T }

let main(): i32 = {
  let value = 20
  read(value) + generic_read(cell: Cell { value: 22 })
}
