let Pair = struct(value: i32)
let value('a: region)(borrow('a) pair: Pair): borrow('a) i32 = { borrow pair.value }

let main(): i32 = {
  let reference = value(Pair(42))
  reference
}
