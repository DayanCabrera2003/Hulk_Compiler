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

/* String introspection helpers used by `s.size()` / `s.charAt(i)` /
   `s.substring(start, len)` method-call lowerings in the codegen. */

/* Length in bytes (also # of chars for ASCII content). */
double __str_size(void* s);

/* Returns a new HulkStr containing the single byte at index `idx`. Indices
   outside [0, len) yield the empty string (no out-of-bounds panic — the
   spec is silent on bounds-check semantics so we choose lenient). */
void* __str_char_at(void* s, double idx);

/* Returns a new HulkStr that copies `len` bytes starting at `start`. If the
   requested range overflows the source string, copying stops at the end. */
void* __str_substring(void* s, double start, double len);

/* Returns 1 if the two HulkStr objects contain the same bytes, 0 otherwise.
   Handles NULL inputs conservatively (NULL == NULL is 1, NULL != non-NULL). */
int __hulk_str_eq(void* a, void* b);

#endif /* HULK_STRINGS_H */
