let result = std.result
let throws = std.error.throws

let fail(comptime error: type)(move error: error): never with(throws(i32), throws(bool)) = {
  throw(error)
}

let main(): i32 = {
  0
}
