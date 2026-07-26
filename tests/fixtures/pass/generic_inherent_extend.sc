let cell(comptime t: type) = struct { value: t }

extend(comptime t: type) cell(t) {
  let new(move value: t): cell(t) = { cell { value: value } }
  let take(move self)(): t = { self.value }
  let replace(self: borrow(mut)(self))(move value: t): () = {
    self.value = value
  }
}

let main(): i32 = {
  let inferred = cell.new(40)
  let left = inferred.take()
  let mut explicit = cell(i32).new(1)
  explicit.replace(2)
  left + explicit.take()
}

test("generic_inherent_extend.sc") {
  main() == 42
}
