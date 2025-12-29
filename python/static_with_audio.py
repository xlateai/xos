import xos

# Configuration
SAMPLE_RATE = 44100  # 44.1 kHz
MAX_SAMPLES_BUFFER_SIZE = 100_000  # Maximum samples in buffer
TARGET_BATCH_SIZE = 4096  # Target batch size for adding samples
AUDIO_MIN = -1.0  # Minimum audio sample value
AUDIO_MAX = 1.0   # Maximum audio sample value

class StaticWithAudio(xos.Application):
    def __init__(self):
        super().__init__()
        self.speaker = None
        self.image_generated = False
        self.tick_count = 0
    
    def setup(self):
        """Initialize the application"""
        xos.print("Static with Audio Generator initialized")
        xos.print("Preparing continuous static video and audio stream...")
        
        # Initialize speaker
        system_type = xos.system.get_system_type()
        
        if system_type == "IOS":
            # On iOS, use device 0 (built-in speaker)
            device_id = 0
        else:
            # On other platforms, let user select
            output_devices = xos.audio.get_output_devices()
            if not output_devices:
                xos.print("Warning: No audio output devices available")
                return
            
            device_names = [dev['name'] for dev in output_devices]
            device_id = xos.dialoguer.select(
                "Select speaker", device_names, default=0
            )
        
        try:
            self.speaker = xos.audio.Speaker(
                device_id=device_id,
                sample_rate=SAMPLE_RATE,
                channels=1  # Mono audio
            )
            xos.print(f"Speaker initialized at {SAMPLE_RATE} Hz")
        except Exception as e:
            xos.print(f"Failed to initialize speaker: {e}")
            self.speaker = None
    
    def tick(self):
        """Generate and display random image, stream random audio"""
        
        self.tick_count += 1
        
        # Get frame dimensions
        width = self.get_width()
        height = self.get_height()
        
        # Generate random visual static EVERY frame (like random_image.py)
        xos.random.uniform_fill(self.frame.array, 0.0, 255.0)
        
        # Print status message only once
        if not self.image_generated:
            xos.print(f"Streaming {width}x{height} random static + audio at {SAMPLE_RATE} Hz")
            self.image_generated = True
        
        # Stream random audio continuously
        if self.speaker is not None:
            try:
                # Check current buffer size
                current_buffer = self.speaker.samples_buffer
                current_buffer_size = current_buffer.shape[0] if hasattr(current_buffer, 'shape') else 0
                
                # Calculate how many samples to add
                # We want to keep the buffer full but not exceed MAX_SAMPLES_BUFFER_SIZE
                samples_to_add = max(
                    MAX_SAMPLES_BUFFER_SIZE - current_buffer_size,
                    TARGET_BATCH_SIZE
                )
                
                # Clamp to reasonable bounds
                samples_to_add = max(TARGET_BATCH_SIZE, min(samples_to_add, MAX_SAMPLES_BUFFER_SIZE))
                
                # Generate random audio samples (white noise)
                # Build a simple list of random floats (more compatible approach)
                audio_samples = []
                for _ in range(samples_to_add):
                    sample = xos.random.uniform(AUDIO_MIN, AUDIO_MAX)
                    audio_samples.append(sample)
                
                # Play the samples
                self.speaker.play_samples(audio_samples)
                
            except Exception as e:
                xos.print(f"Error in audio streaming: {e}")


# Demo code to show how it would be used
if __name__ == "__main__":
    xos.print("🎬 Static with Audio - TV Static Simulator")
    xos.print("Displays continuous random visual static with white noise audio")
    xos.print(f"🔊 Audio: {SAMPLE_RATE} Hz stereo white noise stream")
    
    app = StaticWithAudio()
    app.run()
