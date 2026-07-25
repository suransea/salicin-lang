let defer = std.control.defer

let main(): i32 = {
  let value = defer({ () })
  42
}
