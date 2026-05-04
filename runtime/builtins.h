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
double hulk_log(double x);

/* Pseudo-random Number in [0.0, 1.0). Uses the C standard library rand(). */
double hulk_rand(void);

/* Construct a Range object with the given bounds.
   The Range layout (three doubles: current, max, step) matches the attributes
   of the HULK prelude Range type (session 16). The codegen accesses fields
   by offset; the runtime only allocates the object with the correct size and
   TypeTag. */
void* hulk_range_new(double min, double max, double step);

/* TypeTag for Range objects -- no pointer fields (three doubles). */
extern TypeTag hulk_range_tag;

#endif /* HULK_BUILTINS_H */
