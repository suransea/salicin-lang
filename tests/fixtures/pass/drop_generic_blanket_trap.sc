let Bomb (T: type) = struct { marker: T, divisor: i32 }

extend(T: type) Bomb(T): Drop {
  let drop(borrow(mut) self)(): () = {
    let trapped = 1 / self.divisor
  }}

let main(): i32 = {
  let bomb = Bomb { marker: 42, divisor: 0 }
  0
}
