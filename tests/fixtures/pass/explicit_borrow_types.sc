let pair = struct { left: i32, right: i32 }
let cell(comptime t: type) = struct { value: t }

let read(comptime t: type)(cell: borrow(cell(t))): t
= requires(t is copyable) {
  let alias: borrow(cell(t)) = borrow(cell)
  alias.value
}

let main(): i32 = {
  let mut value = pair { left: 20, right: 2 }
  let before = do {
    let shared: borrow(pair) = borrow(value)
    shared.left
  }
  let mutable: borrow(mut)(pair) = borrow(mut)(value)
  mutable.left = before + 20
  mutable.left + mutable.right + read(cell: cell { value: 1 }) - 1
}

test("explicit_borrow_types.sc") {
  std.test.assert(main() == 42)
}
