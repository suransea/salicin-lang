let identity(T: type)(move value: T): T = value

let main(): i32 = {
  let pending = identity
  pending(42)
}
