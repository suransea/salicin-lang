let Poll = std.async.Poll
let Future = std.async.Future

let main(): i32 = {
  let mut future = async {
    await async { 42 }
  }
  match future.poll()
    { Ready(value) -> value }
    { Pending -> 0 }
}
