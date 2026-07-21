let consume(move value: borrow i32): i32 = value

let main(): i32 = {
  let number = 21
  let reference: borrow i32 = borrow number
  let first = consume(reference)
  first + reference
}
