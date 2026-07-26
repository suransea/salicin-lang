let slice = std.slice

let invalid(): borrow(slice(i32)) = {
  let values = [20, 22]
  borrow(values)
}

let main(): i32 = { invalid().at(0) }
