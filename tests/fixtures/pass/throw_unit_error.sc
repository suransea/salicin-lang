let fail(): i32 with(throws(())) = {
  throw ()
}

let main(): i32 = {
  let result: Result(i32, ()) = try { fail() }
  result ?? 42
}
