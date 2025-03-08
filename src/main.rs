// Import the crate (library) with our modules
use xos::experiments;

fn main() {
    // Command line arguments to decide which demo to run
    let args: Vec<String> = std::env::args().collect();
    
    if args.len() <= 1 {
        println!("Usage: xos [single|four]");
        println!("  single - open a single window with a white pixel");
        println!("  four   - open four windows in quadrants with white pixels");
        return;
    }
    
    match args[1].as_str() {
        "single" => {
            println!("Opening single window...");
            experiments::open_window();
        },
        "four" => {
            println!("Opening four windows...");
            experiments::open_four_windows();
        },
        _ => {
            println!("Unknown option: {}", args[1]);
            println!("Usage: xos [single|four]");
        }
    }
}