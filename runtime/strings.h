#ifndef HULK_STRINGS_H
#define HULK_STRINGS_H

#include "gc.h"
#include <stddef.h>

/* Heap-allocated, immutable HULK string. The payload immediately follows the
   ObjHeader and starts with a size_t length field, then the raw UTF-8 bytes.
   A null terminator is appended for C interop but is not counted in `len`. */
typedef struct HulkStr {
    size_t len;
    char   data[]; /* flexible array: len + 1 bytes allocated */
} HulkStr;

/* TypeTag for all HulkStr objects. Strings contain no pointer fields because
   their payload is raw bytes, not references to other heap objects. */
extern TypeTag hulk_string_tag;

/* Allocate a new HulkStr by copying the null-terminated C string `s`. */
void* hulk_string_new(const char* s);

/* Allocate a new HulkStr whose content is the concatenation of `a` and `b`.
   Both `a` and `b` must be valid HulkStr payload pointers. */
void* hulk_string_concat(void* a, void* b);

/* Convert a double to a HulkStr using the "%g" format (removes trailing
   zeros, uses exponential notation for very large or very small values). */
void* hulk_number_to_string(double n);

#endif /* HULK_STRINGS_H */
