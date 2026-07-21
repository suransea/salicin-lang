let Pair = struct(value: i32)
let value('a: region)(borrow('a) pair: Pair): borrow('a) i32 = { borrow pair.value }

let main(): i32 = {
  let mut pair = Pair(42)
  let reference = value(pair)
  pair.value = 0
  reference
}
