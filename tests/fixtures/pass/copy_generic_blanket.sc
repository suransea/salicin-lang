let Cell(T: type) = struct(value: T)

extend(T: type) Cell(T): Copy
where T: Copy {}

let read_twice(copy cell: Cell(Cell(i32))): i32 = {
  let duplicate = cell
  duplicate.value.value + cell.value.value - 42
}

let main(): i32 = {
  let inner = Cell(42)
  let outer = Cell(inner)
  let duplicate = outer
  read_twice(outer) + duplicate.value.value - 42
}
