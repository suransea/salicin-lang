let option_unit(): Option(()) = ()
let result_unit(): Result((), bool) = ()

let pass_option(): Option(()) = option_unit().try
let pass_result(): Result((), bool) = result_unit().try

let main(): i32 = {
  let option_value = pass_option() ?? ()
  let result_value = pass_result() ?? ()
  42
}
