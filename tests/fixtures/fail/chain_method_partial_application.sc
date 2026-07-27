let option = std.option

let number = struct { value: i32 }

extend(number) {
  let plus(self: borrow(self))(x: i32)(y: i32): i32 = { self.value + x + y }
}

let main(): i32 = {
  let add_last = option(number).some(number { value: 40 })?.plus(1)
  add_last(1) ?? 0
}
