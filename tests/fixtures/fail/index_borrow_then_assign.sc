let index_operator = std.ops.index_operator

let bag = struct { value: i32 }
extend bag: index_operator(i32) {
  let output = i32
  let index(comptime a: access)
    (self: borrow(a)(self))
    (key: i32): borrow(a)(i32) = {
    borrow(a)(self.value)
  }
}

let main(): i32 = {
  let mut bag = bag { value: 1 }
  let item = borrow(bag[0])
  bag[0] = 42
  item
}
