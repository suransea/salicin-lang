let Future = std.async.Future
let Poll = std.async.Poll
let Throws = std.error.Throws

let Ready = struct {}

extend Ready: Future(()) {
  let Output = i32

  let poll(R: region)
    (self: borrow(mut)(R)(Self))
    (): Poll(i32) = {
    Poll(i32).Ready(40)
  }
}

let fail(): () with(Throws(bool)) = {
  throw(true)
}

let main(): i32 = {
  let future = async {
    fail()
    let value = await Ready {}
    value + 2
  }
  0
}
