let result = core.result
let throwing = core.error.throwing

let fail(): i32 with(throwing(bool)) = {
  throw
}

let main(): i32 = { 42 }
