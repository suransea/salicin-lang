let Identity(T: type): type = T

let Factory = trait {
  let Item(T: type): type

  let make(self: borrow(Self))(value: i32): Item(i32)
}

let Cell = struct {}

extend Cell: Factory {
  let Item = Identity

  let make(self: borrow(Self))(value: i32): i32 = { value }
}

let make_i32(T: type)(value: borrow(T)): i32
where T: Factory(Item(U: type) = U) = {
  value.make(42)
}

let main(): i32 = {
  let cell = Cell {}
  make_i32(cell)
}
