let read = trait {
  let read(self: borrow(self))(): i32
}

let leaf = struct { value: i32 }

extend(leaf, read) {
  let read(self: borrow(self))(): i32 = { self.value }
}

let cell(comptime t: type) = struct { value: t }

extend(cell(t), read)
where t: read {
  let read(self: borrow(self))(): i32 = { self.value.read() }
}

let read_cell(comptime t: type)(cell: borrow(cell(t))): i32
where t: read = { cell.read() }

let value = trait {
  let item: type
  let take(move self)(): item
}

extend(cell(t), value) {
  let item = t
  let take(move self)(): t = { self.value }
}

let main(): i32 = {
  let cell = cell { value: leaf { value: 42 } }
  let read = read_cell(cell)
  let leaf = cell.take()
  let wrapped = cell { value: leaf }
  wrapped.read() + read - 42
}

test("trait_generic_blanket_impl.sc") {
  main() == 42
}
