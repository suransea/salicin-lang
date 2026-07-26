let result = std.result
let throws = std.error.throws

let fail(): i32 with(throws(bool)) = {
  throw
}

let main(): i32 = { 42 }
