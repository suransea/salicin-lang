let make(T: type)(value: T): T = value
let subtract(left: i32, right: i32): i32 = left - right

let main(): i32 = {
  let inferred = make(value: subtract(left: 44, right: 2))
  make(T: i32)(value: inferred)
}
