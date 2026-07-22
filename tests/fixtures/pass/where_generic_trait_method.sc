let Convert(To: type) = trait {
  let convert(borrow self)(): To
}

let Value = struct { value: i32 }

extend Value: Convert(i32) {
  let convert(borrow self)(): i32 = { self.value }
}

let convert(T: type)(borrow value: T): i32
where T: Convert(i32) = { value.convert() }

let main(): i32 = {
  let value = Value { value: 42 }
  convert(value)
}
