let option = core.option

let counter = struct { value: i32 }

extend(counter) {
  let reset(self: borrow(mut)(self))(): () = {
    self.value = 0
  }
}

let main(): i32 = {
  let mut counter = option(counter).some(counter { value: 42 })
  counter?.reset()
  0
}
