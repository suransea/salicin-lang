let result = core.result
let throwing = core.error.throwing

let fail: with(throwing(bool))(): i32 = {
  throw(42)
}

let main(): i32 = { 42 }
