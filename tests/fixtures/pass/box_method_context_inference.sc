let box = std.boxed.box

let main(): i32 = {
  let contextual: box(i64) = box.new(42)
  let named = box.new(t: i64)(42)
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
