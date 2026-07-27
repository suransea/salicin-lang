let option = core.option

let box = alloc.boxed.box

let node = struct { value: i32, next: option(box(node)) }

let main(): i32 = {
  let tail = node { value: 42, next: none }
  let head = box.new(tail)
  42
}

test("box_recursive_layout.sc") {
  main() == 42
}
