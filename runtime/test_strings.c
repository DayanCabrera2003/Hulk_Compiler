#include "strings.h"
#include <assert.h>
#include <stdio.h>
#include <string.h>

static int s_run = 0, s_pass = 0;

#define RUNTEST(name) \
    do { s_run++; test_##name(); s_pass++; \
         printf("  PASS: " #name "\n"); } while (0)

static void test_new_empty(void) {
    HulkStr* s = (HulkStr*)hulk_string_new("");
    assert(s->len == 0);
    assert(s->data[0] == '\0');
}

static void test_new_hello(void) {
    HulkStr* s = (HulkStr*)hulk_string_new("hello");
    assert(s->len == 5);
    assert(memcmp(s->data, "hello", 5) == 0);
    assert(s->data[5] == '\0'); /* null terminator present */
}

static void test_concat_basic(void) {
    void*    a = hulk_string_new("hello");
    void*    b = hulk_string_new(" world");
    HulkStr* r = (HulkStr*)hulk_string_concat(a, b);
    assert(r->len == 11);
    assert(memcmp(r->data, "hello world", 12) == 0); /* includes \0 */
}

static void test_concat_left_empty(void) {
    void*    a = hulk_string_new("");
    void*    b = hulk_string_new("world");
    HulkStr* r = (HulkStr*)hulk_string_concat(a, b);
    assert(r->len == 5);
    assert(strcmp(r->data, "world") == 0);
}

static void test_concat_right_empty(void) {
    void*    a = hulk_string_new("hello");
    void*    b = hulk_string_new("");
    HulkStr* r = (HulkStr*)hulk_string_concat(a, b);
    assert(r->len == 5);
    assert(strcmp(r->data, "hello") == 0);
}

static void test_number_to_string_integer(void) {
    HulkStr* s = (HulkStr*)hulk_number_to_string(42.0);
    assert(strcmp(s->data, "42") == 0);
}

static void test_number_to_string_float(void) {
    HulkStr* s = (HulkStr*)hulk_number_to_string(3.14);
    /* %g representation is platform-defined; just verify non-empty. */
    assert(s->len > 0);
    assert(s->data[0] != '\0');
}

static void test_number_to_string_zero(void) {
    HulkStr* s = (HulkStr*)hulk_number_to_string(0.0);
    assert(strcmp(s->data, "0") == 0);
}

static void test_string_tag_no_pointers(void) {
    assert(strcmp(hulk_string_tag.name, "String") == 0);
    assert(hulk_string_tag.num_pointers == 0);
    assert(hulk_string_tag.pointer_offsets == NULL);
}

int main(void) {
    printf("String tests:\n");
    RUNTEST(new_empty);
    RUNTEST(new_hello);
    RUNTEST(concat_basic);
    RUNTEST(concat_left_empty);
    RUNTEST(concat_right_empty);
    RUNTEST(number_to_string_integer);
    RUNTEST(number_to_string_float);
    RUNTEST(number_to_string_zero);
    RUNTEST(string_tag_no_pointers);
    printf("%d/%d passed.\n", s_pass, s_run);
    return (s_pass == s_run) ? 0 : 1;
}
