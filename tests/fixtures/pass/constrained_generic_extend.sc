let Cell(T: type) = struct { value: T }

extend(T: type) Cell(T)
where T: Copy {
  let new(copy value: T): Cell(T) = { Cell { value: value } }
  let duplicate(self: borrow(Self))(): T = {
    let first = self.value
    self.value
  }
}

let read_twice(T: type)(cell: borrow(Cell(T))): T
where T: Copy = { cell.duplicate() }

let main(): i32 = {
  let cell = Cell.new(42)
  read_twice(cell)
}
