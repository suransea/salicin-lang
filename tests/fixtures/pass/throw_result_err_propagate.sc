use core.effects.Throws

let fail(): i32 with(Throws(bool)) = {
  throw true
}

let forward(): i32 with(Throws(bool)) = { fail() }

let main(): i32 = {
  let result: Result(i32, bool) = try { forward() }
  result ?? 42
}
