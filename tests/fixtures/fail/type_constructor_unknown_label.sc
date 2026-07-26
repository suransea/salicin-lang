let cell(comptime t: type) = struct { value: t }

let main(): i32 = {
  let value: cell(element: i32) = cell(i32) { value: 0 }
  value.value
}
