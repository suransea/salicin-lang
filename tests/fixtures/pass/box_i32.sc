let box = alloc.boxed.box

let main(): i32 = {
  box.new(42).read()
}

test("box_i32.sc") {
  std.test.assert(main() == 42)
}
