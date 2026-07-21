let Convert = trait {
  let Output: type
  let convert(borrow self)(): Output
}

let Number = struct(value: i32)

extend Number: Convert {
  let Output = i32
  let convert(borrow self)(): i32 = self.value
}

let main(): i32 = {
  let number = Number(42)
  number.convert()
}
