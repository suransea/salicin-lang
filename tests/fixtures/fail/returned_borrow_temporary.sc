let Pair = struct { value: i32 }
let value(R: region)(pair: borrow(R)(Pair)): borrow(R)(i32) = { borrow(pair.value) }

let main(): i32 = {
  let reference = value(Pair(42))
  reference
}
