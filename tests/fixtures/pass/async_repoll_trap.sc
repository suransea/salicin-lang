let Future = std.async.Future

let main(): i32 = {
  let mut future = async { 42 }
  let first = future.poll()
  let second = future.poll()
  0
}
