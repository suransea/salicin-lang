pub let Iterator = trait {
  let Item: type
  let next(self: borrow(mut)(Self))(): core.Option(Item)
}

pub let IntoIterator = trait {
  let IntoIter: type
  let into_iter(move self)(): IntoIter
}
