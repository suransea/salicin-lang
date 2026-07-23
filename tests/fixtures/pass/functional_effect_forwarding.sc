use std.Option
use std.effect.Unsafe
use std.functional.{Applicative, Functor, Monad}

let unsafe_add_one(value: i32): i32 with(Unsafe) = {
  value + 1
}

let unsafe_next(value: i32): Option(i32) with(Unsafe) = {
  Option(i32).Some(value + 2)
}

let read_option(value: Option(i32)): i32 = {
  value match {
    Some(number) => number,
    None => 0,
  }
}

let main(): i32 = {
  let mapped = unsafe {
    Option(i32).Some(40).map(unsafe_add_one)
  }
  let applied = unsafe {
    let transform: Option((i32): i32 with(Unsafe)) = Option.Some(unsafe_add_one)
    transform.apply(Option(i32).Some(1))
  }
  let chained = unsafe {
    Option(i32).Some(40).flat_map(unsafe_next)
  }
  read_option(mapped) + read_option(applied) + read_option(chained) - 43
}
