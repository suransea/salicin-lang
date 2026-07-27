let result = core.result
let throwing = core.error.throwing

let fail(comptime error: type)(move error: error): never with(throwing(i32), throwing(bool)) = {
  throw(error)
}

let main(): i32 = {
  0
}
