let Convert(To: type) = trait {
  let convert(self: borrow(Self))(): To
}

let Value = struct { value: i32 }

extend Value: Convert(i32) {
  let convert(self: borrow(Self))(): i32 = { self.value }
}

let convert(T: type)(value: borrow(T)): i32
where T: Convert(i32) = { value.convert() }

let main(): i32 = {
  let value = Value { value: 42 }
  convert(value)
}
