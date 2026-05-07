#include "builtins.h"
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* Internal Range layout. The codegen never reads these fields directly;
   they are accessed through the HULK Range type's attribute methods. */
typedef struct HulkRange {
    double current;
    double max;
    double step;
} HulkRange;

TypeTag hulk_range_tag = { "Range", 0, NULL };

void hulk_print(void* obj) {
    if (obj == NULL) {
        printf("null\n");
        return;
    }
    ObjHeader*  hdr = HULK_HEADER(obj);
    const char* tag = hdr->tag->name;
    if (strcmp(tag, "String") == 0) {
        HulkStr* s = (HulkStr*)obj;
        printf("%s\n", s->data);
    } else {
        /* User-defined types without a toString() method fall back to this.
           Session 15 codegen will emit a MethodCall to toString() first
           when the type declares one; this branch handles the no-toString case. */
        printf("<%s>\n", tag);
    }
}

void hulk_print_number(double n) {
    /* Match the %g format used by hulk_number_to_string for consistency. */
    printf("%g\n", n);
}

void hulk_print_bool(int b) {
    printf("%s\n", b ? "true" : "false");
}

double hulk_sqrt(double x) { return sqrt(x);  }
double hulk_sin(double x)  { return sin(x);   }
double hulk_cos(double x)  { return cos(x);   }
double hulk_exp(double x)  { return exp(x);   }
double hulk_log(double base, double x) { return log(x) / log(base); }

double hulk_rand(void) {
    /* Dividing by RAND_MAX+1.0 yields [0.0, 1.0) -- never exactly 1.0. */
    return (double)rand() / ((double)RAND_MAX + 1.0);
}

void* hulk_range_new(double min, double max) {
    HulkRange* r = (HulkRange*)hulk_alloc(&hulk_range_tag, sizeof(HulkRange));
    /* Start one step before min so the first next() call yields min. */
    r->current = min - 1.0;
    r->max     = max;
    r->step    = 1.0;
    return (void*)r;
}

/* Advance the range iterator and return 1 if still in range, 0 otherwise. */
int __range_next(void* range) {
    HulkRange* r = (HulkRange*)range;
    r->current += r->step;
    return r->current < r->max;
}

/* Return the current value of the range iterator. */
double __range_current(void* range) {
    HulkRange* r = (HulkRange*)range;
    return r->current;
}

/* String concatenation helper used by the @ operator.
   Both arguments must be valid HulkStr heap objects. */
void* __hulk_concat(void* a, void* b) {
    return hulk_string_concat(a, b);
}

/* ── Vector (numeric element array) ──────────────────────────────────── */

typedef struct HulkVec {
    size_t  len;
    size_t  cap;
    size_t  pos;    /* iteration cursor; starts at (size_t)-1 */
    double* data;   /* C-heap double array; not GC-traced */
} HulkVec;

TypeTag hulk_vec_tag = { "Vector", 0, NULL };

void* __vec_new(double initial_cap) {
    size_t cap = (size_t)(initial_cap > 0 ? initial_cap : 4);
    HulkVec* v = (HulkVec*)hulk_alloc(&hulk_vec_tag, sizeof(HulkVec));
    v->len  = 0;
    v->cap  = cap;
    v->pos  = (size_t)-1;
    v->data = (double*)malloc(cap * sizeof(double));
    return (void*)v;
}

void* __vec_push(void* vec, double elem) {
    HulkVec* v = (HulkVec*)vec;
    if (v->len == v->cap) {
        v->cap  *= 2;
        v->data  = (double*)realloc(v->data, v->cap * sizeof(double));
    }
    v->data[v->len++] = elem;
    return vec;
}

double __vec_get(void* vec, double idx) {
    HulkVec* v = (HulkVec*)vec;
    return v->data[(size_t)idx];
}

/* Advance the vector iterator; returns 1 if there is a current element. */
int __vec_next(void* vec) {
    HulkVec* v = (HulkVec*)vec;
    v->pos++;
    return v->pos < v->len;
}

/* Return the element at the current iterator position. */
double __vec_current(void* vec) {
    HulkVec* v = (HulkVec*)vec;
    return v->data[v->pos];
}
