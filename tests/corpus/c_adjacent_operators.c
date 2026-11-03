/* Two operators written apart stay apart: together they would read as
 * another operator, or `/` and `*` as the start of a comment. */
int adjacent(int a, int b, int *q)
{
    int r = a / *q;
    r = a/ *q;
    r = a - -b;
    r = a + +b;
    r = a - --b;
    r = a + ++b;
    r = - -a;
    r = - --a;
    r = + +a;
    r = a & &b != 0;
    r = a ? -b : - -b;
    r = a < -b;
    r = a > -b;
    return r;
}
