let convert = trait {
  let output: type
}

let number = struct { value: i32 }

extend number: convert() {}

let main(): i32 = { 0 }
