let cell(comptime t: type) = struct { value: t }
let consume(comptime t: type)(move value: t): i32 = { 21 }

let main(): i32 = {
  let cell = cell(i32) { value: 42 }
  consume(cell) + consume(cell)
}
