let fail(): i32 with(throws(bool)) = {
  throw 42
}

let main(): i32 = 42
