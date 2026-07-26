let cell(comptime t: type) = struct { value: t }
let holder = struct { cell: cell(i32) }

extend(comptime t: type) cell(t) {
  let take(move self)(): t = { self.value }
}

let main(): i32 = {
  let holder = holder { cell: cell { value: 42 } }
  holder.cell.take()
}

test("generic_inherent_existing_instance.sc") {
  main() == 42
}
