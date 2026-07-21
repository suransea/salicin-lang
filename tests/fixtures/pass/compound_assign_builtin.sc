let Cell = struct(value: i32)

let main(): i32 = {
  let mut add = Cell(40)
  add.value += 2

  let mut sub = 44
  sub -= 2

  let mut mul = 21
  mul *= 2

  let mut div = 84
  div /= 2

  let mut rem = 85
  rem %= 43

  let mut array = [40, 0]
  array[0] += 2

  (add.value + sub + mul + div + rem + array[0]) / 6
}
