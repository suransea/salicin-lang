let number = struct { raw: i32 }

extend number {
  let reset(self: borrow(mut)(self))(value: i32): () = {
    self.raw = value
  }
  let add(self: borrow(self))(amount: i32): i32 = { self.raw + amount }
  let value(self: borrow(self))(): i32 = { self.raw }
  let value(): i32 = { 2 }
}

let main(): i32 = {
  let mut number = number { raw: 0 }
  number.reset(number)(40)
  let sum = number.add(number)(2)
  let method = number.value(self: number)()
  let temporary = number.value(self: number { raw: 42 })()
  let associated = number.value()
  sum + method + temporary + associated - 84
}

test("qualified_inherent_method.sc") {
  main() == 42
}
