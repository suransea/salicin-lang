let add(value: borrow i32)(amount: i32): i32 = { value + amount }

let main(): i32 = {
  let number = 41
  let reference: borrow i32 = borrow number
  let pending = add(reference)
  pending(1)
}
