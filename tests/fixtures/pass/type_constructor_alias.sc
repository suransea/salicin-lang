let Cell(T: type) = struct { value: T }

let Family(T: type): type = Cell(T)
let Constructor: (T: type): type = Cell
let Scalar = i32

let main(): Scalar = {
  let left: Family(i32) = Family(i32) { value: 41 }
  let right = Constructor(i32) { value: 1 }
  left.value + right.value
}
