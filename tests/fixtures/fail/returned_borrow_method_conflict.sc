let Cell = struct { value: i32 }

extend Cell {
  let get(R: region)(self: borrow(R)(Self))(): borrow(R)(i32) = { borrow(self.value) }
}

let main(): i32 = {
  let mut cell = Cell { value: 42 }
  let reference = cell.get()
  cell.value = 0
  reference
}
