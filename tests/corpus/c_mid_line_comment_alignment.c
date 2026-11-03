/* A trailing comment is aligned with the ones on the lines around it; a
   comment with code after it on its line is not trailing, unless that
   code only closes what the comment is in. */
#define SIZE (16U /* pid */ + 1U) /* bytes */
#define COUNT 4U /* entries */

static const int table[] = {
    1 /* a */,
    22 /* bb */,
    333, /* ccc */
};

int g(int a);

void f(int a)
{
    struct s { int a; } /* c */;
    int value; /* the value */
    do { /* d */ } while (a); /* t */
    a = 2; /* two */
    if (a) { /* bare */ }
    a = g(a /* arg */) + 1; /* sum */
    a = g( /* first */ a) + a /* x */ + 1; /* three */
}
