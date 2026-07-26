let into_iterator = std.iter.into_iterator

let iterable = struct {}
let iter = struct {}

extend iterable: into_iterator {
  let iter = iter
  let into_iter(move self)(): iter = { iter {} }}

let main(): i32 = {
  for iterable {} { value ->
    value
  }
  0
}
