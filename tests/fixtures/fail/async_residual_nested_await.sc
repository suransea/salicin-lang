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

let main(): i32 = {
  let future = async {
    let first = await make_step()
    let second = await Step { value: first + 1 }
    second + 1
  }
  0
}
