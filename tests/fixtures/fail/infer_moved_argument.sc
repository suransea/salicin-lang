let Cell(T: type) = struct { value: T }
let consume(T: type)(move value: T): i32 = { 21 }

let main(): i32 = {
  let cell = Cell(i32) { value: 42 }
  consume(cell) + consume(cell)
}
