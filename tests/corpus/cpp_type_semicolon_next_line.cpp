// A type's `;` after a block comment stays on the comment's line only
// where the source has it there.
namespace ns {
class W {} /* on the next line */
;
class X {} /* on this line */;
struct Y {} // after a line comment
;
}

void f()
{
    struct S {} /* on the next line */
    ;
    struct T {} /* on this line */ ;
}
