use core.Result
use core.effects.Throws

let fail(): i32 with(Throws(bool)) = {
  throw
}

let main(): i32 = { 42 }
