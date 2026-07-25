let Future = std.async.Future
let Poll = std.async.Poll

let Ask = effect {
  let ask(): i32
}

let First = struct {
  value: i32,
}

let Second = struct {
  value: i32,
}

extend First: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    Poll(i32).Ready(self.value)
  }
}

extend Second: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    Poll(i32).Ready(self.value)
  }
}

let main(): i32 = {
  let future = async {
    if true {
      await First { value: Ask.ask() }
    } else {
      await Second { value: Ask.ask() }
    }
  }
  0
}
