/* An empty statement on the line after a preprocessor line stays on a
   line of its own, after each kind of directive. */
#include <stdio.h>
;
#define A 1
;
#define B(x) \
    ((x) + 1)
;
#undef A
;
#pragma pack(1)
;
#ifdef B
int y;
#endif
;
#line 20
;;
int z;
