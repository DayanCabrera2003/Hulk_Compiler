#include "strings.h"
#include <string.h>
#include <stdio.h>

TypeTag hulk_string_tag = { "String", 0, NULL, NULL };

void* hulk_string_new(const char* s) {
    size_t   len = strlen(s);
    HulkStr* str = (HulkStr*)hulk_alloc(&hulk_string_tag,
                                         sizeof(size_t) + len + 1);
    str->len = len;
    memcpy(str->data, s, len + 1); /* copy including null terminator */
    return (void*)str;
}

void* hulk_string_concat(void* a, void* b) {
    HulkStr* sa  = (HulkStr*)a;
    HulkStr* sb  = (HulkStr*)b;
    size_t   tot = sa->len + sb->len;
    HulkStr* res = (HulkStr*)hulk_alloc(&hulk_string_tag,
                                         sizeof(size_t) + tot + 1);
    res->len = tot;
    memcpy(res->data,           sa->data, sa->len);
    memcpy(res->data + sa->len, sb->data, sb->len + 1); /* copy null too */
    return (void*)res;
}

void* hulk_number_to_string(double n) {
    char buf[64];
    /* %g drops trailing zeros and switches to exponential notation when
       the exponent is outside [-4, precision). Matches HULK's print format. */
    snprintf(buf, sizeof(buf), "%g", n);
    return hulk_string_new(buf);
}

/* ------------ String introspection ----------------------------------- */

double __str_size(void* s) {
    return s ? (double)((HulkStr*)s)->len : 0.0;
}

void* __str_char_at(void* s, double idx) {
    if (s == NULL) return hulk_string_new("");
    HulkStr* str = (HulkStr*)s;
    size_t i = (size_t)idx;
    if (idx < 0 || i >= str->len) return hulk_string_new("");
    char buf[2] = { str->data[i], '\0' };
    return hulk_string_new(buf);
}

void* __str_substring(void* s, double start, double len) {
    if (s == NULL) return hulk_string_new("");
    HulkStr* str = (HulkStr*)s;
    if (start < 0) start = 0;
    if (len < 0) len = 0;
    size_t st = (size_t)start;
    size_t ln = (size_t)len;
    if (st >= str->len) return hulk_string_new("");
    if (st + ln > str->len) ln = str->len - st;
    HulkStr* result = (HulkStr*)hulk_alloc(&hulk_string_tag,
                                           sizeof(size_t) + ln + 1);
    result->len = ln;
    for (size_t i = 0; i < ln; i++) result->data[i] = str->data[st + i];
    result->data[ln] = '\0';
    return (void*)result;
}

int __hulk_str_eq(void* a, void* b) {
    /* Pointer identity short-circuit. */
    if (a == b) return 1;
    if (!a || !b) return 0;
    HulkStr* sa = (HulkStr*)a;
    HulkStr* sb = (HulkStr*)b;
    /* Different lengths → cannot be equal. */
    if (sa->len != sb->len) return 0;
    return memcmp(sa->data, sb->data, sa->len) == 0;
}
