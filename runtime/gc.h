#ifndef HULK_GC_H
#define HULK_GC_H

#include <stddef.h>

/* Describes a managed type: name, number of pointer-typed fields, and their
   byte offsets from the start of the object payload. The GC reads this to
   trace references during the mark phase.

   `parent` points at the TypeTag for the direct base type (NULL for the
   root). `__hulk_is` walks this chain to answer "is X a subtype of Y?". */
typedef struct TypeTag {
    const char*       name;
    size_t            num_pointers;
    size_t*           pointer_offsets;
    struct TypeTag*   parent;
} TypeTag;

/* Prepended to every heap-allocated object. Threads all allocations through
   an intrusive singly-linked list so the sweeper can visit every object. */
typedef struct ObjHeader {
    TypeTag*          tag;
    size_t            size;   /* total bytes: sizeof(ObjHeader) + payload */
    int               mark;
    struct ObjHeader* next;
} ObjHeader;

/* Derive payload pointer from a header pointer, and vice versa. */
#define HULK_PAYLOAD(hdr) ((void*)((ObjHeader*)(hdr) + 1))
#define HULK_HEADER(pay)  ((ObjHeader*)(pay) - 1)

/* GC bookkeeping globals -- defined in gc.c. */
extern ObjHeader* __hulk_alloc_list;
extern size_t     __hulk_alloc_bytes;
extern size_t     __hulk_gc_threshold;

/* Shadow stack: flat array of void* holding every live reference-typed value.
   Sized for 4096 simultaneous live references; enough for any realistic HULK
   program without dynamic growth. */
#define HULK_SHADOW_STACK_CAPACITY 4096
extern void*  __hulk_shadow_stack[HULK_SHADOW_STACK_CAPACITY];
extern size_t __hulk_shadow_top;

/* Allocate a managed object of `payload_size` bytes (not including the header).
   Triggers a GC cycle if the heap threshold is exceeded before allocating. */
void* hulk_alloc(TypeTag* tag, size_t payload_size);

/* Run one full mark-and-sweep cycle. */
void hulk_gc(void);

/* Push `val` onto the shadow stack. Called by generated code when a
   reference-typed variable is introduced. */
void hulk_shadow_push(void* val);

/* Pop one slot from the shadow stack. Called at scope exit. */
void hulk_shadow_pop(void);

#endif /* HULK_GC_H */
