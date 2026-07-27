let defer = core.control.defer

let main(): i32 = {
  let value = defer { () }
  42
}
