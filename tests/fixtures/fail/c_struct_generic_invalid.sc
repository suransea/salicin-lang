let wrapper(comptime t: type) = struct(c) {
  value: t,
}

let main(): i32 = {
  if size_of(wrapper(bool)) == 1 { 0 } else { 0 }
}
