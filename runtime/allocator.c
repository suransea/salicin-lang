#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

#if defined(__GNUC__) || defined(__clang__)
#define SALICIN_WEAK __attribute__((weak))
#else
#define SALICIN_WEAK
#endif

static void salicin_invalid_layout(void) {
    abort();
}

SALICIN_WEAK void *salicin_alloc(uint64_t size, uint64_t align) {
    if (align == 0 || (align & (align - 1)) != 0 || size > SIZE_MAX || align > SIZE_MAX) {
        salicin_invalid_layout();
    }

    size_t native_size = size == 0 ? 1 : (size_t)size;
    size_t native_align = (size_t)align;
    void *pointer;
    if (native_align <= _Alignof(max_align_t)) {
        pointer = malloc(native_size);
    } else {
        if (native_size > SIZE_MAX - (native_align - 1)) {
            salicin_invalid_layout();
        }
        size_t rounded = (native_size + native_align - 1) & ~(native_align - 1);
        pointer = aligned_alloc(native_align, rounded);
    }
    if (pointer == NULL) {
        abort();
    }
    return pointer;
}

SALICIN_WEAK void salicin_dealloc(void *pointer, uint64_t size, uint64_t align) {
    if (pointer == NULL || align == 0 || (align & (align - 1)) != 0 || size > SIZE_MAX ||
        align > SIZE_MAX) {
        salicin_invalid_layout();
    }
    free(pointer);
}
