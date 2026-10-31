void f(void)
{
    enum E { A } // the enum
    ;
    struct S { int a; } // the struct
    ;
    class W {} // the class
    ;
    class X // declared ahead
    ;
    struct T { int a; }
    /* on a line of its own */
    ;
    struct U { int a; } /* one */;
    struct V { int a; } /* one */ /* two */ ;
}
