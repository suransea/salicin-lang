let Result = std.Result
let Throws = std.effect.Throws

let fail(): i32 with(Throws(())) = {
  throw(())
}

let main(): i32 = {
  let result: Result(())(i32) = try { fail() }
  result ?? 42
}
