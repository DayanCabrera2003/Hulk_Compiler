#ifndef HULK_BUILTINS_H
#define HULK_BUILTINS_H

#include "gc.h"
#include "strings.h"

/* Print a reference-typed value (String or user-defined type).
   Inspects the TypeTag to decide the output format:
   - "String" -> prints the UTF-8 content followed by a newline.
   - anything else -> prints the type name surrounded by angle brackets. */
void hulk_print(void* obj);

/* Print a Number (f64) value. Codegen calls this directly for Number
   expressions to avoid boxing a value type into a heap object. */
void hulk_print_number(double n);

/* Print a Boolean value ("true" or "false"). */
void hulk_print_bool(int b);

/* Mathematical builtins. All take and return double to match HULK's Number type. */
double hulk_sqrt(double x);
double hulk_sin(double x);
double hulk_cos(double x);
double hulk_exp(double x);
double hulk_log(double base, double x);

/* Pseudo-random Number in [0.0, 1.0). Uses the C standard library rand(). */
double hulk_rand(void);

/* Construct a Range object [min, max) with step 1.0.
   Internally, current is set to min-1 so the first next() call yields min. */
void* hulk_range_new(double min, double max);

/* Advance the range iterator; returns 1 while in range, 0 when done. */
int __range_next(void* range);

/* Return the current value of the range iterator. */
double __range_current(void* range);

/* TypeTag for Range objects -- no pointer fields (three doubles). */
extern TypeTag hulk_range_tag;

/* String concatenation used by the @ operator. */
void* __hulk_concat(void* a, void* b);

/* Vector (numeric element array) operations. */
void*  __vec_new(double initial_cap);
void*  __vec_push(void* vec, double elem);
double __vec_get(void* vec, double idx);
int    __vec_next(void* vec);
double __vec_current(void* vec);
double __vec_size(void* vec);

extern TypeTag hulk_vec_tag;

/* Runtime type test: returns 1 if `obj`'s TypeTag equals `target` or any of
   its ancestors (walking the `parent` chain). Returns 0 if `obj` is NULL or
   no ancestor matches. */
int __hulk_is(void* obj, TypeTag* target);

/* Runtime checked downcast: returns `obj` unchanged if `__hulk_is(obj,target)`
   is true, otherwise prints a diagnostic to stderr and aborts the program. */
void* __hulk_as(void* obj, TypeTag* target);

#endif /* HULK_BUILTINS_H */
