let View(T: type)(R: region): type = borrow(R)(T)

let Lend = trait {
  let Item(R: region): type
}

let Cell = struct { value: i32 }

extend Cell: Lend {
  let Item = View(i32)
}

let require_i64(T: type)(move value: T): ()
where T: Lend(Item(R: region) = borrow(R)(i64)) = {}

let main(): () = {
  require_i64(Cell { value: 42 })
}
