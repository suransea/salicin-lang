let View(T: type)(A: access)(R: region): type = borrow(A)(R)(T)

let Lend = trait {
  let Item(A: access)(R: region): type

  let view(A: access, R: region)
    (self: borrow(A)(R)(Self))(): Item(A)(R)
}

let Cell = struct { value: i32 }

extend Cell: Lend {
  let Item = View(i32)

  let view(A: access, R: region)
    (self: borrow(A)(R)(Self))(): borrow(A)(R)(i32) = {
    borrow(A)(self.value)
  }
}

let read(value: borrow(i32)): i32 = { value }

let main(): i32 = {
  let mut cell = Cell { value: 40 }
  let before = do {
    let value = cell.view(shared)()
    read(value)
  }
  do {
    let value = cell.view(mut)()
    value = before + 2
  }
  let final_value = cell.view(shared)()
  read(final_value)
}

test("gat_borrow_family.sc") {
  main() == 42
}
