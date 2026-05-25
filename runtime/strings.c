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
