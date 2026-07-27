let cell = struct { value: i32 }

extend(cell) {
  let clash(self: borrow(self))(move other: cell): i32 = { self.value + other.value }
}

let main(): i32 = {
  let cell = cell { value: 21 }
  cell.clash(cell)
}
