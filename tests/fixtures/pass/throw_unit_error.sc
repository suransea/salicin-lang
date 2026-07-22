use core.Result
use core.effects.Throws

let fail(): i32 with(Throws(())) = {
  throw(())
}

let main(): i32 = {
  let result: Result(i32, ()) = try { fail() }
  result ?? 42
}
