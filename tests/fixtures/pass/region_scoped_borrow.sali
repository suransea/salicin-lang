let read('a: region)(borrow('a) value: i32): i32 = {
  let alias: borrow('a) i32 = borrow value
  alias
}

let generic_read('a: region, T: type)(borrow('a) cell: Cell(T)): T
where T: Copy = {
  let alias: borrow('a) Cell(T) = borrow cell
  alias.value
}

let Cell(T: type) = struct(value: T)

let main(): i32 = {
  let value = 20
  read(value) + generic_read(cell: Cell(22))
}
