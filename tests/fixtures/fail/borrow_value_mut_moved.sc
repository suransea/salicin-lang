let consume(move value: borrow(mut)(i32)): i32 = { value }

let main(): i32 = {
  let mut number = 42
  let reference: borrow(mut)(i32) = borrow(mut)(number)
  let value = consume(reference)
  value + reference
}
