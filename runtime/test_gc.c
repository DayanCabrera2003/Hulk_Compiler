#include "gc.h"
#include <assert.h>
#include <stdio.h>

/* Type with no pointer fields (behaves like a Number wrapper). */
static TypeTag s_num_tag = { "TestNumber", 0, NULL, NULL };

/* Type with one pointer field at offset 0. */
static size_t s_one_offset[] = { 0 };
static TypeTag s_node_tag = { "TestNode", 1, s_one_offset, NULL };

static int s_run = 0, s_pass = 0;

#define RUNTEST(name) \
    do { s_run++; test_##name(); s_pass++; \
         printf("  PASS: " #name "\n"); } while (0)

static void test_alloc_basic(void) {
    void* obj = hulk_alloc(&s_num_tag, sizeof(double));
    assert(obj != NULL);
    assert(__hulk_alloc_bytes >= sizeof(ObjHeader) + sizeof(double));
}

static void test_alloc_increments_bytes(void) {
    size_t before = __hulk_alloc_bytes;
    hulk_alloc(&s_num_tag, 8);
    assert(__hulk_alloc_bytes > before);
}

static void test_shadow_push_pop(void) {
    void*  obj = hulk_alloc(&s_num_tag, 8);
    size_t top = __hulk_shadow_top;
    hulk_shadow_push(obj);
    assert(__hulk_shadow_top == top + 1);
    hulk_shadow_pop();
    assert(__hulk_shadow_top == top);
}

static void test_gc_frees_unreachable(void) {
    /* Allocate without pushing onto shadow stack -- object is unreachable. */
    hulk_alloc(&s_num_tag, 8);
    size_t bytes_before_gc = __hulk_alloc_bytes;
    hulk_gc();
    assert(__hulk_alloc_bytes < bytes_before_gc);
}

static void test_gc_keeps_reachable(void) {
    void* obj = hulk_alloc(&s_num_tag, 8);
    hulk_shadow_push(obj);
    size_t after_alloc = __hulk_alloc_bytes;
    hulk_gc();
    /* Object must survive because it is on the shadow stack. */
    assert(__hulk_alloc_bytes == after_alloc);
    hulk_shadow_pop();
}

static void test_gc_traces_pointer_fields(void) {
    /* Clear unreachable leftovers from previous tests so the baseline is exact. */
    hulk_gc();

    void** obj1 = (void**)hulk_alloc(&s_node_tag, sizeof(void*));
    void*  obj2 = hulk_alloc(&s_num_tag, 8);
    *obj1 = obj2; /* obj2 is reachable only through obj1's pointer field */

    hulk_shadow_push((void*)obj1);
    size_t after = __hulk_alloc_bytes;
    hulk_gc();
    assert(__hulk_alloc_bytes == after); /* both must survive */
    assert(*obj1 == obj2);
    hulk_shadow_pop();
}

int main(void) {
    printf("GC tests:\n");
    RUNTEST(alloc_basic);
    RUNTEST(alloc_increments_bytes);
    RUNTEST(shadow_push_pop);
    RUNTEST(gc_frees_unreachable);
    RUNTEST(gc_keeps_reachable);
    RUNTEST(gc_traces_pointer_fields);
    printf("%d/%d passed.\n", s_pass, s_run);
    return (s_pass == s_run) ? 0 : 1;
}
