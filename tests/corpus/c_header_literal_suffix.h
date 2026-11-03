/* A C header is often compiled as C++ too, where a string written
   together with a name after it is one literal with a suffix. */
#define NAME "n"
#define L "l"
#define PREFIX(x) "p" x

static const char *a = "name[" NAME "]";
static const char *b = "a" L "b";
static const char *c = PREFIX("q") NAME;
