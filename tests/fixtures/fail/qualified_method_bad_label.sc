let Number = struct(value: i32)

extend Number {
  let read(borrow self)(): i32 = { self.value }
}

let main(): i32 = {
  let number = Number(42)
  Number.read(receiver: number)()
}
