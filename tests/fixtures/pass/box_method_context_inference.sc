let Box = std.boxed.Box

let main(): i32 = {
  let contextual: Box(i64) = Box.new(42)
  let named = Box.new(T: i64)(42)
  let left = contextual.into_inner()
  let right = named.into_inner()
  if left + right == 84 {
    42
  } else {
    0
  }
}

test("box_method_context_inference.sc") {
  main() == 42
}
