let Convert = trait {
  let Output: type
}

let Number = struct(value: i32)

extend Number: Convert {
}

let main(): i32 = 0
