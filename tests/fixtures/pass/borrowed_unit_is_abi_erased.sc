let observe(borrow value: ()): () = { value }

let main(): i32 = {
  let unit = ()
  observe(unit)
  42
}
