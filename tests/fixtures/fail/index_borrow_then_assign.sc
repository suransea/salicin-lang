let Index = std.ops.Index

let Bag = struct { value: i32 }
extend Bag: Index(i32) {
  let Output = i32
  let index(A: access)
    (self: borrow(A)(Self))
    (key: i32): borrow(A)(i32) = {
    borrow(A)(self.value)
  }
}

let main(): i32 = {
  let mut bag = Bag { value: 1 }
  let item = borrow(bag[0])
  bag[0] = 42
  item
}
