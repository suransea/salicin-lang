let result = std.result
let throwing = std.error.throwing

let fail(): i32 with(throwing(bool)) = {
  throw
}

let main(): i32 = { 42 }
