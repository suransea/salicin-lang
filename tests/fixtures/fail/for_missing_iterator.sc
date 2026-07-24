use std.iter.IntoIterator

let Iterable = struct {}
let Iter = struct {}

extend Iterable: IntoIterator {
  let IntoIter = Iter
  let into_iter(move self)(): Iter = { Iter {} }}

let main(): i32 = {
  for Iterable {} { value ->
    value
  }
  0
}
