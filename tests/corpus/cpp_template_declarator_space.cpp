// A name after the `>` that closes a template's arguments is spaced from
// it, in a parameter list as in a declaration.
#include <map>
#include <vector>

template <typename T> T first(std::vector<T> items);
void f(std::vector<int> v, const std::map<int, int> &m, std::vector<std::vector<int>> vv);
void g(std::vector<int> v) {}
auto h = [](std::vector<int> q) { return q; };
int k(int x) { return static_cast<int>(x); }
