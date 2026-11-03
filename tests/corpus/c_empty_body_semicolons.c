/* An empty statement after a statement whose body is itself empty is
   not joined to that body; the ones after it on its line stay there. */
void f(int a)
{
    for (;;) ; ;;
    if (a) ; ;
    while (a) ; ;
    if (a) f(a); else ; ;
L: ; ;
    f(a);;
}
