let Lend = trait {
  let Item(A: access): type
}

let require(T: type)(move value: T): ()
where T: Lend(Item(R: region) = i32) = {}

let main(): () = {}
