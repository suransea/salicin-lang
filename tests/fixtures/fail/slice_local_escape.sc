let Slice = std.Slice

let invalid(): borrow(Slice(i32)) = {
  let values = [20, 22]
  borrow(values)
}

let main(): i32 = { invalid().at(0) }
