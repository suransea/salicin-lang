let Cell = struct { value: i32 }

extend Cell {
  let clash(self: borrow(Self))(move other: Cell): i32 = { self.value + other.value }
}

let main(): i32 = {
  let cell = Cell { value: 21 }
  cell.clash(cell)
}
