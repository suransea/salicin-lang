let future = std.async.future
let poll = std.async.poll

let ask = effect {
  let ask(): i32
}

let step = struct {
  value: i32,
}

extend step: future(()) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    poll(i32).ready(self.value)
  }
}

let make_step(): step with(ask) = {
  step { value: ask.ask() }
}

let program(value: borrow(mut)(i32)): i32 = {
  let future = async {
    let amount = await make_step()
    value = value + amount
    value
  }
  value = 0
  42
}

let main(): i32 = {
  let mut value = 2
  program(value)
}
