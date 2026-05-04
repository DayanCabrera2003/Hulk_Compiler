#include "gc.h"
#include <stdlib.h>
#include <string.h>
#include <stdio.h>

#define GC_INITIAL_THRESHOLD (1024u * 1024u)  /* 1 MiB before first collection */
#define GC_GROWTH_FACTOR     2u

ObjHeader* __hulk_alloc_list   = NULL;
size_t     __hulk_alloc_bytes  = 0;
size_t     __hulk_gc_threshold = GC_INITIAL_THRESHOLD;
void*      __hulk_shadow_stack[HULK_SHADOW_STACK_CAPACITY];
size_t     __hulk_shadow_top   = 0;

/* Recursively mark an object and all objects reachable from its pointer fields.
   Early return on NULL or already-marked objects prevents infinite loops in
   cycles -- mark-and-sweep handles cycles correctly without extra bookkeeping. */
static void mark(void* payload) {
    if (payload == NULL) return;
    ObjHeader* hdr = HULK_HEADER(payload);
    if (hdr->mark) return;
    hdr->mark = 1;
    char* base = (char*)payload;
    for (size_t i = 0; i < hdr->tag->num_pointers; i++) {
        void* child = *(void**)(base + hdr->tag->pointer_offsets[i]);
        mark(child);
    }
}

void hulk_gc(void) {
    /* Mark phase: trace from every slot on the shadow stack. */
    for (size_t i = 0; i < __hulk_shadow_top; i++) {
        mark(__hulk_shadow_stack[i]);
    }

    /* Sweep phase: remove unmarked objects; clear mark bits on survivors. */
    ObjHeader** cursor = &__hulk_alloc_list;
    while (*cursor != NULL) {
        ObjHeader* obj = *cursor;
        if (obj->mark) {
            obj->mark = 0;
            cursor = &obj->next;
        } else {
            *cursor = obj->next;
            __hulk_alloc_bytes -= obj->size;
            free(obj);
        }
    }

    /* Grow the threshold proportionally to the surviving live set so
       collection frequency decreases as the program's working set grows. */
    size_t new_threshold = __hulk_alloc_bytes * GC_GROWTH_FACTOR;
    if (new_threshold < GC_INITIAL_THRESHOLD) {
        new_threshold = GC_INITIAL_THRESHOLD;
    }
    __hulk_gc_threshold = new_threshold;
}

void* hulk_alloc(TypeTag* tag, size_t payload_size) {
    size_t total = sizeof(ObjHeader) + payload_size;

    if (__hulk_alloc_bytes + total > __hulk_gc_threshold) {
        hulk_gc();
        if (__hulk_alloc_bytes + total > __hulk_gc_threshold) {
            fprintf(stderr, "hulk: out of memory after GC\n");
            abort();
        }
    }

    ObjHeader* hdr = (ObjHeader*)malloc(total);
    if (hdr == NULL) {
        fprintf(stderr, "hulk: malloc failed\n");
        abort();
    }

    hdr->tag  = tag;
    hdr->size = total;
    hdr->mark = 0;
    hdr->next = __hulk_alloc_list;
    __hulk_alloc_list  = hdr;
    __hulk_alloc_bytes += total;
    memset(HULK_PAYLOAD(hdr), 0, payload_size);
    return HULK_PAYLOAD(hdr);
}

void hulk_shadow_push(void* val) {
    if (__hulk_shadow_top >= HULK_SHADOW_STACK_CAPACITY) {
        fprintf(stderr, "hulk: shadow stack overflow\n");
        abort();
    }
    __hulk_shadow_stack[__hulk_shadow_top++] = val;
}

void hulk_shadow_pop(void) {
    if (__hulk_shadow_top == 0) {
        fprintf(stderr, "hulk: shadow stack underflow\n");
        abort();
    }
    --__hulk_shadow_top;
}
