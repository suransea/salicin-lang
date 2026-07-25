let Poll = std.async.Poll
let Future = std.async.Future

let Number = struct {}
let Flag = struct {}

extend Number: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    Poll(i32).Ready(42)
  }
}

extend Flag: Future(()) {
  let Output = bool

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(bool) = {
    Poll(bool).Ready(true)
  }
}

let main(): i32 = {
  let future = async {
    if true {
      await Number {}
    } else {
      await Flag {}
    }
  }
  0
}
