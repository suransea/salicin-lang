let Convert = trait {
  let Output: type
  let convert(self: borrow(Self))(): Output
}

let Number = struct { value: i32 }

extend Number: Convert {
  let Output = i32
  let convert(self: borrow(Self))(): i32 = { self.value }}

let main(): i32 = {
  let number = Number { value: 42 }
  number.convert()
}
