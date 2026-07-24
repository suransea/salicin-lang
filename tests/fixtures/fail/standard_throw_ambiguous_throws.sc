let Result = std.Result
let Throws = std.effect.Throws

let fail(Error: type)(move error: Error): Never with(Throws(i32), Throws(bool)) = {
  throw(error)
}

let main(): i32 = {
  0
}
