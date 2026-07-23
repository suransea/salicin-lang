let observe(value: borrow(())): () = { value }

let main(): i32 = {
  let unit = ()
  observe(unit)
  42
}
