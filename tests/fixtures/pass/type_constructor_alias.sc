let Cell(T: type) = struct(value: T)

let Family(T: type): type = Cell(T)
let Constructor: (T: type): type = Cell
let Scalar = i32

let main(): Scalar = {
  let left: Family(i32) = Family(i32)(41)
  let right = Constructor(1)
  left.value + right.value
}
