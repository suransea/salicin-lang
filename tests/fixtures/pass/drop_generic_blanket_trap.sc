let bomb(comptime t: type) = struct { marker: t, divisor: i32 }

extend(bomb(t), droppable) {
  let drop(self: borrow(mut)(self))(): () = {
    let trapped = 1 / self.divisor
  }
}

let main(): i32 = {
  let bomb = bomb { marker: 42, divisor: 0 }
  0
}

test("drop_generic_blanket_trap.sc") {
  main() == 42
}
