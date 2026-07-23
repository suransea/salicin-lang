let Pair = struct { left: i32, right: i32 }
let Cell(T: type) = struct { value: T }

let read(T: type)(cell: borrow(Cell(T))): T
where T: Copy = {
  let alias: borrow(Cell(T)) = borrow(cell)
  alias.value
}

let main(): i32 = {
  let mut value = Pair { left: 20, right: 2 }
  let before = do {
    let shared: borrow(Pair) = borrow(value)
    shared.left
  }
  let mutable: borrow(mut)(Pair) = borrow(mut)(value)
  mutable.left = before + 20
  mutable.left + mutable.right + read(cell: Cell { value: 1 }) - 1
}
