let cell(comptime t: type) = struct { value: t }

let main(): i32 = {
  let cell = cell(i32, bool) { value: 42 }
  cell.value
}
