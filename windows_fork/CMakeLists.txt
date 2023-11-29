cmake_minimum_required(VERSION 3.4.1)

project(win_fork)

file(GLOB_RECURSE lib_srcs include/*.h src/*.c)

add_library(win_fork SHARED ${lib_srcs})
target_include_directories(win_fork PUBLIC include)
target_link_libraries(win_fork ntdll)
target_compile_definitions(win_fork PRIVATE IMPLEMENT_FORK_API)

add_library(win_fork_static STATIC ${lib_srcs})
set_target_properties(win_fork_static PROPERTIES OUTPUT_NAME win_fork)
target_include_directories(win_fork_static PUBLIC include)
target_link_libraries(win_fork_static ntdll)
target_compile_definitions(win_fork_static PRIVATE IMPLEMENT_FORK_API)

add_executable(test_win_fork tests/test.c ${lib_srcs})
add_test(
    NAME test_win_fork
    COMMAND test_win_fork
)
target_link_libraries(test_win_fork ntdll)
target_include_directories(test_win_fork PUBLIC include)
