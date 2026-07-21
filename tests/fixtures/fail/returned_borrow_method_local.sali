let Cell = struct(value: i32)

extend Cell {
  let get('a: region)(borrow('a) self)(): borrow('a) i32 = borrow self.value
}

let bad('a: region)(borrow('a) seed: i32): borrow('a) i32 = {
  let cell = Cell(seed)
  cell.get()
}

let main(): i32 = 42
