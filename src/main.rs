use clap::{Parser, Subcommand};
use std::time::Duration;

use xos::experiments;
use xos::viewport;
use xos::waveform;
use xos::audio;

#[derive(Parser)]
#[command(name = "xos")]
#[command(about = "Experimental OS Windows Manager", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Open a single window with a white pixel at center
    Screen,
    
    /// Open four windows in quadrants with white pixels
    Quad,
    
    /// Open the XOS viewport with grid
    View,
    
    /// Record a 3-second audio clip from device 0 and print stats
    Record,

    Waveform,
}

fn main() {
    // Print audio device information at startup
    let audio_devices = audio::devices();
    println!("XOS Audio: {} device(s) detected", audio_devices.len());

    audio::print_devices();
    
    let cli = Cli::parse();
    
    // Execute the command
    match cli.command {
        Commands::Screen => {
            println!("Opening single window...");
            experiments::open_window();
        }
        Commands::Quad => {
            println!("Opening four windows...");
            experiments::open_four_windows();
        }
        Commands::View => {
            println!("Opening viewport...");
            viewport::open_viewport();
        }
        Commands::Record => {
            println!("Recording 3-second audio clip from device 0...");
            record_audio_sample();
        }
        Commands::Waveform => {
            println!("Opening audio waveform visualization...");
            waveform::open_waveform();
        }
    }
}

fn record_audio_sample() {
    // Get device with index 0 (first device)
    let devices = audio::devices();
    let device = devices.get(0).unwrap();
    
    // Create a new listener just for this recording
    let listener = match audio::AudioListener::new(&device.device_cpal, 3.0) {
        Ok(listener) => listener,
        Err(e) => {
            println!("Error creating listener: {}", e);
            return;
        }
    };
    
    println!("Recording started... Please make some noise!");
    
    // Record for 3 seconds
    // let samples = listener.record_for(Duration::from_secs(3));

    // record for 3 seconds by sleeping for 3s in main thread
    let _ = listener.record();
    std::thread::sleep(Duration::from_secs(3));
    let _ = listener.pause();

    let samples = listener.get_samples();
    
    // Print stats
    println!("Recording complete!");
    println!("Captured {} samples", samples.len());
    println!("Average value: {:.6}", listener.buffer().get_average());
    println!("RMS value: {:.6}", listener.buffer().get_rms());
    println!("Peak value: {:.6}", listener.buffer().get_peak());
    
    // Print histogram of values (simple visualization)
    println!("\nAmplitude distribution:");
    let buffer = listener.buffer();
    let samples = buffer.get_samples();
    
    // Create 10 buckets from -1.0 to 1.0
    let mut buckets = [0; 10];
    for sample in samples {
        // Map sample from [-1.0, 1.0] to [0, 9]
        let bucket = ((sample + 1.0) * 4.5).floor() as usize;
        let bucket = bucket.min(9);
        buckets[bucket] += 1;
    }
}