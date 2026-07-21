# Salicin minimal runtime

`allocator.c` provides the default implementation of Salicin's replaceable raw allocator ABI:

```c
void *salicin_alloc(uint64_t size, uint64_t align);
void salicin_dealloc(void *pointer, uint64_t size, uint64_t align);
```

Both definitions are weak on Clang/GCC targets. A platform runtime or embedding application can
provide strong definitions with the same ABI. `salicin_alloc` must return a non-null pointer aligned
to `align`, or terminate; `salicin_dealloc` must receive the same layout that created the allocation.
The layout alignment must be a non-zero power of two. Invalid layouts and allocation failure abort.

The compiler embeds this source and links it for `salic build` and `salic run`. `emit-ir` leaves calls
to the two ABI symbols unresolved so another linker pipeline can supply its own runtime.
