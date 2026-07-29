let option = core.option

let read(value: borrow(i32)): i32 = { value }
let positive(value: borrow(i32)): bool = { read(value) > 0 }

let invalid(): option(borrow(i32)) = {
  let values: array(i32)(1) = [42]
  values.find(positive)
}

let main(): i32 = { invalid()!! }
