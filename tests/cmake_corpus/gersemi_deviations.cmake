install(TARGETS a b c d e f DESTINATION x)
find_program(HIP HINTS "${ROOT}/bin" NO_CACHE)
# A no-break space is an argument, not a separator; gersemi rejects it.
set(A   b)
# A parenthesized group holding a comment that is not the last condition
# word, or follows a word that is not `AND` or `OR`, is expanded so the
# comment keeps its place; gersemi indents some of the words differently.
if((A # c
) B)
endif()
if(A AND # c1
 (B # c2
) C)
endif()
if(A IN_LIST (B # c
))
endif()
if(A AND # c1
 ${X} (B # c2
))
endif()
# gersemi keeps the first argument beside `set(` before a bracket comment
# and a comment.
set(A #[[x]] # c
 b)
# A bracket comment after an argument, touching it or not, puts the
# arguments one per line; gersemi keeps them on one line.
set(A a#[[note]] b)
message(STATUS "a"#[[c]] b)
list(APPEND A [[a]]#[[c]] b)
