let Result = std.Result
let Throws = std.effect.Throws

let fail(): i32 with(Throws(bool)) = {
  throw
}

let main(): i32 = { 42 }
