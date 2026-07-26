let choose = effect {
  let choose(): i32
}

let main(): i32 = {
  choose.handle choose { (resume) ->
      resume(20);
      resume(22)
    } action {
      choose.choose()
    }
}
