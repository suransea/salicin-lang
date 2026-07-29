let read(value: borrow(i32)): i32 = { value }
let positive(value: borrow(i32)): bool = { read(value) > 0 }

let main(): i32 = {
  let mut values: array(i32)(2) = [20, 22]
  let found = values.find(positive)!!
  values.reverse()
  read(found)
}
