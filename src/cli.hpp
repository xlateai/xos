#pragma once
#include 

class AppCLI {
public:
    explicit AppCLI(bool verbose = false);
    
    void process_file(const std::string& filename);
    void interactive_mode();
    
private:
    bool m_verbose;
    
    void print_status(const std::string& message);
};