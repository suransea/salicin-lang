let pair = struct { left: i32, right: i32 }

let increment_both(left: borrow(mut)(i32), right: borrow(mut)(i32)): () = {
  left = left + 1
  right = right + 1
}

let main(): i32 = {
  let mut pair = pair { left: 19, right: 21 }
  increment_both(pair.left, pair.right)
  pair.left + pair.right
}

test("disjoint_mut_field_borrows.sc") {
  std.test.assert(main() == 42)
}
