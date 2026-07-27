let convert = trait {
  let output: type
  let convert(self: borrow(self))(): output
}

let number = struct { value: i32 }

extend(number, convert) {
  let output = i32
  let convert(self: borrow(self))(): i32 = { self.value }}

let main(): i32 = {
  let number = number { value: 42 }
  number.convert()
}

test("trait_associated_output.sc") {
  main() == 42
}
