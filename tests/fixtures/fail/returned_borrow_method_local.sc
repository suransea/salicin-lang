let Cell = struct { value: i32 }

extend Cell {
  let get('a: region)(self: borrow('a)(Self))(): borrow('a)(i32) = { borrow(self.value) }
}

let bad('a: region)(seed: borrow('a)(i32)): borrow('a)(i32) = {
  let cell = Cell { value: seed }
  cell.get()
}

let main(): i32 = { 42 }
