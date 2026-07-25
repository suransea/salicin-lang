let Slice = std.Slice

let write(target: borrow(mut)(i32))(value: i32): () = {
  target = value
}

let main(): i32 = {
  let mut values: Array(i32)(3) = [9, 10, 20]
  do {
    let slice: borrow(mut)(Slice(i32)) = borrow(mut)(values)
    let mut iterator = slice.iter(mut)()
    do {
      let item = iterator.next()!!
      write(item)(14)
    }
    do {
      let item = iterator.next()!!
      write(item)(14)
    }
    do {
      let item = iterator.next()!!
      write(item)(14)
    }
  }
  values[0] + values[1] + values[2]
}
