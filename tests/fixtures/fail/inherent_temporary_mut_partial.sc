let Counter = struct { value: i32 }

extend Counter {
  let set(self: borrow(mut)(Self))(value: i32)(extra: i32): i32 = {
    self.value = value
    self.value + extra
  }
}

let main(): i32 = {
  let set = Counter { value: 0 }.set(40)
  set(2)
}
