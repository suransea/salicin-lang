use core.effects.Throws

let fail(): i32 with(Throws(bool)) = {
  throw 42
}

let main(): i32 = { 42 }
