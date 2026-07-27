let future = std.async.future
let poll = std.async.poll

let ask = effect {
  let ask(): i32
}

let first = struct {
  value: i32,
}

let second = struct {
  value: i32,
}

extend(first, future(())) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    poll(i32).ready(self.value)
  }
}

extend(second, future(())) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    poll(i32).ready(self.value)
  }
}

let main(): i32 = {
  let future = async {
    let value = 1
    let reference: borrow(i32) = borrow(value)
    let awaited = await if true {
      first { value: ask.ask() }
    } else {
      second { value: ask.ask() }
    }
    reference + awaited
  }
  0
}
