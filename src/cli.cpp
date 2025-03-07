#include "cli.hpp"
#include <fmt/color.h>
#include <fmt/core.h>
#include 

AppCLI::AppCLI(bool verbose) 
    : m_verbose(verbose) {
    if (m_verbose) {
        fmt::print("Initializing CLI in verbose mode\n");
    }
}

void AppCLI::process_file(const std::string& filename) {
    print_status(fmt::format("Processing file: {}", filename));
    // Implement file processing logic here
}

void AppCLI::interactive_mode() {
    print_status("Starting interactive mode");
    
    fmt::print(fg(fmt::color::green) | fmt::emphasis::bold, 
               "Welcome to the CLI App Interactive Mode!\n");
    fmt::print("Type 'exit' to quit\n\n");
    
    std::string command;
    while (true) {
        fmt::print(fg(fmt::color::blue), "> ");
        std::getline(std::cin, command);
        
        if (command == "exit" || command == "quit") {
            break;
        } else if (command == "help") {
            fmt::print("Available commands:\n");
            fmt::print("  help - Show this help\n");
            fmt::print("  exit - Exit the application\n");
        } else if (!command.empty()) {
            fmt::print("Unknown command: {}\n", command);
        }
    }
    
    print_status("Exiting interactive mode");
}

void AppCLI::print_status(const std::string& message) {
    if (m_verbose) {
        fmt::print(fg(fmt::color::yellow), "[STATUS] {}\n", message);
    }
}