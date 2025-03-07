cmake_minimum_required(VERSION 3.16)
project(cli_app VERSION 0.1.0 LANGUAGES CXX)

# Set C++ standard
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

# Add CLI11 for argument parsing (assuming vcpkg or another package manager)
find_package(CLI11 CONFIG REQUIRED)

# Add fmt for formatting
find_package(fmt CONFIG REQUIRED)

# Add executable
add_executable(cli_app
  src/main.cpp
  src/cli.cpp
)

# Include directories
target_include_directories(cli_app
  PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/src
  PUBLIC
    ${CMAKE_CURRENT_SOURCE_DIR}/include
)

# Link libraries
target_link_libraries(cli_app
  PRIVATE
    CLI11::CLI11
    fmt::fmt
)

# Install
install(TARGETS cli_app
  RUNTIME DESTINATION bin
)