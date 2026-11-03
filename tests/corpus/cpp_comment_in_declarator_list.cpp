// The same in C++, inside a namespace, a class and a function.
namespace ns {
int g = 1, // first
    h;
class W {
    int a, // a
        b;
    static const int k = 1, // k
        l = 2;
};
}

int f()
{
    auto s = 1, // s
        t = 2;
    int u = 3 // before the comma
        , v = 4;
    return s + t + u + v;
}
