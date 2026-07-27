let vec = alloc.vec.vec

let bomb = struct {}

extend(bomb, droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    unsafe {
      raw_trap()
    }
  }
}

let main(): i32 = {
  let mut values: vec(bomb) = vec(bomb).new()
  values.push(bomb {})
  0
}

test("vec_zst_resource_drop_trap.sc") {
  main() == 42
}
