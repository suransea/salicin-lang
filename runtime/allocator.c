#include <stddef.h>
#include <stdint.h>
#include <errno.h>
#include <stdlib.h>

#if defined(_WIN32)
#include <io.h>
#define salicin_write _write
#else
#include <unistd.h>
#define salicin_write write
#endif

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

static int salicin_test_report_descriptor(void) {
    static int initialized = 0;
    static int descriptor = -1;
    if (initialized) {
        return descriptor;
    }
    initialized = 1;
    const char *value = getenv("SALICIN_TEST_REPORT_FD");
    if (value == NULL || *value == '\0') {
        return -1;
    }
    char *end = NULL;
    long parsed = strtol(value, &end, 10);
    if (end == value || *end != '\0' || parsed < 0 || parsed > INT32_MAX) {
        return -1;
    }
    descriptor = (int)parsed;
    return descriptor;
}

static int salicin_write_all(int descriptor, const uint8_t *data, uint64_t length) {
    while (length != 0) {
        size_t chunk = length > SIZE_MAX ? SIZE_MAX : (size_t)length;
        ssize_t written = salicin_write(descriptor, data, chunk);
        if (written < 0 && errno == EINTR) {
            continue;
        }
        if (written <= 0) {
            return -1;
        }
        data += (size_t)written;
        length -= (uint64_t)written;
    }
    return 0;
}

static void salicin_store_u64_le(uint8_t *output, uint64_t value) {
    for (unsigned index = 0; index < 8; ++index) {
        output[index] = (uint8_t)(value >> (index * 8));
    }
}

SALICIN_WEAK int32_t sali_host_test_report(
    uint64_t index,
    uint8_t status,
    uint8_t has_message,
    const uint8_t *data,
    uint64_t length
) {
    int descriptor = salicin_test_report_descriptor();
    if (status > 2 || has_message > 1 || (status != 1 && has_message != 0) ||
        (has_message != 0 && data == NULL)) {
        return -1;
    }
    /*
     * Embedders may execute compiler-generated test IR directly rather than
     * through `salic test`. In that case there is no report consumer, so keep
     * the native summary status usable while discarding structured frames.
     */
    if (descriptor < 0) {
        return 0;
    }
    uint8_t header[24] = {
        'S', 'L', 'T', '1',
        0, 0, 0, 0, 0, 0, 0, 0,
        status, has_message, 0, 0,
        0, 0, 0, 0, 0, 0, 0, 0,
    };
    salicin_store_u64_le(header + 4, index);
    salicin_store_u64_le(header + 16, length);
    if (salicin_write_all(descriptor, header, sizeof(header)) != 0) {
        return -1;
    }
    if (status == 1 && has_message != 0 &&
        salicin_write_all(descriptor, data, length) != 0) {
        return -1;
    }
    return 0;
}
