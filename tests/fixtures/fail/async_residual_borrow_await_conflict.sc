let Future = std.async.Future
let Poll = std.async.Poll

let Ask = effect {
  let ask(): i32
}

let Step = struct {
  value: i32,
}

extend Step: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    Poll(i32).Ready(self.value)
  }
}

let make_step(): Step with(Ask) = {
  Step { value: Ask.ask() }
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
