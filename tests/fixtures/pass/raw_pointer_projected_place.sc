let Pair = struct {
  left: i32,
  right: i32,
}

let main(): i32 = {
  let mut pair = Pair { left: 18, right: 20 }
  let mut values = [10, 10]
  do {
    let pointer = Ptr(mut)(borrow(mut)(pair))
    unsafe {
      (*pointer).left = (*pointer).left + 2
      (*pointer).right = (*pointer).right + 2
    }
  }
  do {
    let pointer = Ptr(mut)(borrow(mut)(values))
    unsafe {
      (*pointer)[0] = (*pointer)[0] + 1
      (*pointer)[1] = (*pointer)[1] + 1
    }
  }
  pair.left + pair.right + values[0] + values[1] - 22
}
