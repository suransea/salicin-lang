let identity(comptime t: type)(move value: t): t = { value }

let main(): i32 = {
  let pending = identity
  pending(42)
}
