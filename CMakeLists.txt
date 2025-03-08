cmake_minimum_required(VERSION 3.10)
project(SimpleViewport)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# Add SFML
find_package(SFML 2.5 COMPONENTS graphics window system REQUIRED)

add_executable(SimpleViewport src/main.cpp)
target_link_libraries(SimpleViewport sfml-graphics sfml-window sfml-system)