let Cell (T: type) = struct { value: T }

extend(T: type) Cell(T) {
  let identity(U: type)(borrow self)(move value: U): U = { value }
}

let main(): i32 = {
  let cell = Cell { value: 0 }
  cell.identity(i32)(42)
}
