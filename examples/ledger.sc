// A complete effectful batch-processing program over the frozen M0 core. The
// process exits with 42 after applying four valid transactions, or 1 on overdraft.

let option = core.option
let iterator = core.iter.iterator
let into_iterator = core.iter.into_iterator
let owned_item = core.iter.owned_item

let transaction = enum {
  credit(i32),
  debit(i32),
}

let overdraft = effect {
  let reject(): never
}

let ledger = struct {
  balance: i32,
  processed: i32,
}

let account = trait {
  let credit(self: borrow(mut)(self))(amount: i32): ()
  let debit(self: borrow(mut)(self))(amount: i32): ()
  let snapshot(self: borrow(self))(): i32
}

extend(ledger, account) {
  let credit(self: borrow(mut)(self))(amount: i32): () = {
    self.balance = self.balance + amount
    self.processed = self.processed + 1
  }

  let debit(self: borrow(mut)(self))(amount: i32): () = {
    self.balance = self.balance - amount
    self.processed = self.processed + 1
  }

  let snapshot(self: borrow(self))(): i32 = {
    if self.processed == 4 { self.balance } else { 0 }
  }
}

let batch = struct {
  index: i32,
}

extend(batch, iterator) {
  let item = owned_item(transaction)

  let next(comptime r: region)(self: borrow(mut)(r)(self))(): option(transaction) = {
    let transaction: option(transaction) = match self.index
      { 0 -> some(transaction.credit(30)) }
      { 1 -> some(transaction.debit(8)) }
      { 2 -> some(transaction.credit(25)) }
      { 3 -> some(transaction.debit(5)) }
      { _ -> none }
    self.index = self.index + 1
    transaction
  }
}

extend(batch, into_iterator) {
  let iter = batch

  let into_iter(move self)(): batch = {
    self
  }
}

let count_batch(move batch: batch): i32 = {
  let mut count = 0
  for batch { _ ->
    count = count + 1
  }
  count
}

let apply: with(overdraft)(ledger: borrow(mut)(ledger))(move transaction: transaction): () = {
  match transaction
    { credit(amount) -> ledger.credit(amount) }
    { debit(amount) ->
      if amount > ledger.balance {
        overdraft.reject()
      } else {
        ledger.debit(amount)
      }
    }
}

let process: with(overdraft)(move batch: batch): i32 = {
  let mut ledger = ledger { balance: 0, processed: 0 }
  for batch { transaction ->
    apply(ledger)(transaction)
  }
  ledger.snapshot()
}

let main(): i32 = {
  let balance = overdraft.handle reject { () -> 1 } action {
    process(batch { index: 0 })
  }
  let count = count_batch(batch { index: 0 })
  if balance == 42 && count == 4 { 42 } else { 1 }
}
