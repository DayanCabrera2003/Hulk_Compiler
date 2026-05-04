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
double hulk_log(double x)  { return log(x);   }

double hulk_rand(void) {
    /* Dividing by RAND_MAX+1.0 yields [0.0, 1.0) -- never exactly 1.0. */
    return (double)rand() / ((double)RAND_MAX + 1.0);
}

void* hulk_range_new(double min, double max, double step) {
    HulkRange* r = (HulkRange*)hulk_alloc(&hulk_range_tag, sizeof(HulkRange));
    r->current = min;
    r->max     = max;
    r->step    = step;
    return (void*)r;
}
