#include <CLI/CLI.hpp>
#include <fmt/core.h>
#include "cli.hpp"
#include "cli_app/version.hpp"

int main(int argc, char** argv) {
    CLI::App app{"Modern C++ CLI Application"};
    app.set_version_flag("--version", CLI_APP_VERSION_STRING);
    
    std::string input;
    app.add_option("-i,--input", input, "Input file to process");
    
    bool verbose = false;
    app.add_flag("-v,--verbose", verbose, "Enable verbose output");
    
    CLI11_PARSE(app, argc, argv);
    
    AppCLI cli(verbose);
    
    if (!input.empty()) {
        cli.process_file(input);
    } else {
        cli.interactive_mode();
    }
    
    return 0;
}