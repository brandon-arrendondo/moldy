cmake_minimum_required(VERSION 3.20)
PROJECT( moldy_demo  VERSION 1.0 LANGUAGES C CXX )   # trailing comment

# A comment
set(SOURCES src/a.c src/b.c src/c.c src/d.c src/e.c src/f.c src/g.c src/h.c src/i.c)
set( SHORT a b )
add_executable(demo ${SOURCES})
target_link_libraries(demo PRIVATE m pthread PUBLIC foo::bar INTERFACE baz)
if(WIN32 AND NOT MSVC)
message(STATUS "windows")
elseif( UNIX )
  message( STATUS "unix" )
else()
    message(FATAL_ERROR "unknown platform")
endif()



foreach(item IN LISTS SOURCES)
  message(${item})
endforeach()
function(my_func arg1 arg2)
  set(${arg1} "${arg2}" PARENT_SCOPE)
endfunction()
macro(m)
endmacro()
install(TARGETS demo RUNTIME DESTINATION bin LIBRARY DESTINATION lib ARCHIVE DESTINATION lib/static)
set(MULTI
    a # comment a
    b
)
string(REPLACE "a" "b" OUT "${IN}")
set(BRACKET [[raw
  text]])
#[[ bracket
comment ]]
