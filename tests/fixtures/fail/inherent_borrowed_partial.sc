let number = struct { value: i32 }

extend number {
  let plus(self: borrow(self))(x: i32)(y: i32): i32 = { self.value + x + y }
}

let main(): i32 = {
  let number = number { value: 40 }
  let add_last = number.plus(1)
  add_last(1)
}
