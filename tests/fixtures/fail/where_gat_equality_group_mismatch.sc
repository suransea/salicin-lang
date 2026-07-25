let Lend = trait {
  let Item(A: access)(R: region): type
}

let require(T: type)(move value: T): ()
where T: Lend(Item(A: access, R: region) = borrow(A)(R)(i32)) = {}

let main(): () = {}
