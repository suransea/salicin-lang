let Cell(T: type) = struct { value: T }

extend(T: type) Cell(T) {
  let identity(U: type)(self: borrow(Self))(move value: U): U = { value }
}

let main(): i32 = {
  let cell = Cell { value: 0 }
  cell.identity(i32)(42)
}

test("generic_extend_generic_member.sc") {
  main() == 42
}
