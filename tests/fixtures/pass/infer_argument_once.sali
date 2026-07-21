let identity(T: type)(move value: T): T = value

let tick(borrow(mut) count: i32): i32 = {
  count = count + 1
  42
}

let main(): i32 = {
  let mut count = 0
  let value = identity(tick(count))
  if count == 1 { value } else { 0 }
}
