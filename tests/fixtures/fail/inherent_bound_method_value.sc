let number = struct { raw: i32 }

extend number {
  let value(self: borrow(self))(): i32 = { self.raw }
}

let main(): i32 = {
  let number = number { value: 42 }
  let bound = number.value
  bound()
}
