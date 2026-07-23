let Cell = struct { value: i32 }

extend Cell {
  let get(R: region)(self: borrow(R)(Self))(): borrow(R)(i32) = { borrow(self.value) }
}

let bad(R: region)(seed: borrow(R)(i32)): borrow(R)(i32) = {
  let cell = Cell { value: seed }
  cell.get()
}

let main(): i32 = { 42 }
