let poll = std.async.poll
let future = std.async.future

let number = struct {}
let flag = struct {}

extend(number, future(())) {
  let output = i32

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(i32) = {
    poll(i32).ready(42)
  }
}

extend(flag, future(())) {
  let output = bool

  let poll(comptime r: region)
    (self: borrow(mut)(r)(self))
    (): poll(bool) = {
    poll(bool).ready(true)
  }
}

let main(): i32 = {
  let future = async {
    if true {
      await number {}
    } else {
      await flag {}
    }
  }
  0
}
