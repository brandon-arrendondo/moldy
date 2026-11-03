/* GCC reads raw strings in C in its GNU modes (gnu17 is its default), so
   R and a string written together would be one raw string, not a macro
   and a string. */
#define R "M:"
#define LR "N:"

const char *s = "a" R "(y)";
const char *t = "b" LR "(z)";
const char *r = R"(raw)";
const char *u = u8R"x(raw)x";
