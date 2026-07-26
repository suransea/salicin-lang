let pair = struct { value: i32 }
let value(comptime r: region)(pair: borrow(r)(pair)): borrow(r)(i32) = { borrow(pair.value) }

let main(): i32 = {
  let reference = value(pair { value: 42 })
  reference
}
