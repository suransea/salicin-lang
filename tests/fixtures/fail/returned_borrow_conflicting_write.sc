let Pair = struct { value: i32 }
let value(R: region)(pair: borrow(R)(Pair)): borrow(R)(i32) = { borrow(pair.value) }

let main(): i32 = {
  let mut pair = Pair { value: 42 }
  let reference = value(pair)
  pair.value = 0
  reference
}
