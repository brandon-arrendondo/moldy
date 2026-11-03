/* A comment inside a case label keeps the label's `:` on its line; a
   line comment there ends the line, and the `:` follows on the next. */
void f(int a)
{
    switch (a) {
    case 1/* one */:
        break;
    case '{' /*}*/: {
        break;
    }
    case /* before */ 2 :
    case 3 // three
        :
        break;
    default /* none */ :
        break;
    }
}
