let Read = trait {
  let read(borrow self)(): i32
}

let Cell(T: type) = struct(value: T)

extend Cell(i32): Read {
  let read(borrow self)(): i32 = { self.value }
}

let main(): i32 = {
  let cell = Cell(i32)(42)
  cell.read()
}
