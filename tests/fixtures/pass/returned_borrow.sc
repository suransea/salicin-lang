let pair = struct { left: i32, right: i32 }
let holder(comptime t: type) = struct { value: t }

let right_view = trait {
  let view(comptime r: region)(self: borrow(r)(self))(): borrow(r)(i32)
}

let left(comptime r: region)(pair: borrow(r)(pair)): borrow(r)(i32) = { borrow(pair.left) }

let left_mut(comptime r: region)
  (pair: borrow(mut, r)(pair)): borrow(mut, r)(i32) = { borrow(mut)(pair.left) }

let forward(comptime r: region)(pair: borrow(r)(pair)): borrow(r)(i32) = { left(pair) }

let same(comptime r: region, comptime t: type)(value: borrow(r)(t)): borrow(r)(t) = { borrow(value) }

let forwarded_method(comptime r: region)(pair: borrow(r)(pair)): borrow(r)(i32) = { pair.right_method() }

let inferred_left(pair: borrow(pair)): borrow(i32) = { borrow(pair.left) }

let inferred_same(comptime t: type)(value: borrow(t)): borrow(t) = { borrow(value) }

let inferred_forward(comptime r: region)(pair: borrow(r)(pair)): borrow(r)(i32) = { inferred_left(pair) }

extend(pair) {
  let right_ref(comptime r: region)(pair: borrow(r)(pair)): borrow(r)(i32) = { borrow(pair.right) }

  let right_method(comptime r: region)(self: borrow(r)(self))(): borrow(r)(i32) = { borrow(self.right) }

  let left_mut_method(comptime r: region)
    (self: borrow(mut, r)(self))(): borrow(mut, r)(i32) = { borrow(mut)(self.left) }

  let inferred_right(self: borrow(self))(): borrow(i32) = { borrow(self.right) }

  let inferred_left_mut(self: borrow(mut)(self))(): borrow(mut)(i32) = { borrow(mut)(self.left) }
}

extend(holder(t)) {
  let get(comptime r: region)(self: borrow(r)(self))(): borrow(r)(t) = { borrow(self.value) }
}

extend(pair, right_view) {
  let view(comptime r: region)(self: borrow(r)(self))(): borrow(r)(i32) = { borrow(self.right) }
}

let main(): i32 = {
  let mut pair_value = pair { left: 20, right: 0 }
  let holder_value = holder { value: 0 }
  let before = do {
    let reference = forward(pair_value)
    let generic = same(value: pair_value)
    let associated = pair.right_ref(pair_value)
    let method = pair_value.right_method()
    let qualified = pair.right_method(self: pair_value)()
    let forwarded = forwarded_method(pair_value)
    let generic_method = holder_value.get()
    let trait_method = pair_value.view()
    let inferred = inferred_left(pair_value)
    let inferred_generic = inferred_same(value: pair_value)
    let inferred_forwarded = inferred_forward(pair_value)
    let inferred_method = pair_value.inferred_right()
    reference + generic.right + associated + method + qualified + forwarded + generic_method +
      trait_method + inferred + inferred_generic.right + inferred_forwarded + inferred_method - 40
  }
  do {
    let reference = pair_value.inferred_left_mut()
    reference = 22
  }
  before + pair_value.left
}

test("returned_borrow.sc") {
  main() == 42
}
