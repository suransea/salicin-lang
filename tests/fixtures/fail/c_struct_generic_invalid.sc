let Wrapper(T: type) = struct(c) {
  value: T,
}

let main(): i32 = {
  if size_of(Wrapper(bool)) == 1 { 0 } else { 0 }
}
