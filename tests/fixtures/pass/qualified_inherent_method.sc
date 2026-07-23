let Number = struct { raw: i32 }

extend Number {
  let reset(self: borrow(mut)(Self))(value: i32): () = {
    self.raw = value
  }
  let add(self: borrow(Self))(amount: i32): i32 = { self.raw + amount }
  let value(self: borrow(Self))(): i32 = { self.raw }
  let value(): i32 = { 2 }
}

let main(): i32 = {
  let mut number = Number { raw: 0 }
  Number.reset(number)(40)
  let sum = Number.add(number)(2)
  let method = Number.value(self: number)()
  let temporary = Number.value(self: Number { raw: 42 })()
  let associated = Number.value()
  sum + method + temporary + associated - 84
}
