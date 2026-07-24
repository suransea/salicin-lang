let Choose = effect {
  let choose(): i32
}

let main(): i32 = {
  Choose.handle choose { (resume) ->
      resume(20);
      resume(22)
    } action {
    Choose.choose()
  }
}
