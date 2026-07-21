let identity(P: passing, T: type)(P value: T): T = { value }

let main(): i32 = {
  let number = 20
  let moved = identity(move, i32)(number)
  moved + number
}
