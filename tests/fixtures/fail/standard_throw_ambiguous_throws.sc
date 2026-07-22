use core.effects.Throws

let fail(): Never with(Throws(i32), Throws(bool)) = {
  throw 0
}

let main(): i32 = {
  0
}
