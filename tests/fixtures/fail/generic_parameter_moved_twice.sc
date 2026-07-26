let invalid(comptime t: type)(value: t): t = {
  let first = value
  value
}

let main(): i32 = { 42 }
