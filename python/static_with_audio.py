import xos

# Configuration
SAMPLE_RATE = 44100  # 44.1 kHz
BUFFER_PAD_SECONDS = 1.0  # Maximum buffer size in seconds
MAX_SAMPLES_BUFFER_SIZE = int(SAMPLE_RATE * BUFFER_PAD_SECONDS)  # Maximum samples in buffer (1 second)
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
            # On iOS, use default output device (built-in speaker)
            device_id = None
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
            xos.print(f"🔊 Speaker: {self.speaker.name}")
            xos.print(f"   Sample rate: {SAMPLE_RATE} Hz")
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
            xos.print(f"Buffer pad: {BUFFER_PAD_SECONDS}s ({MAX_SAMPLES_BUFFER_SIZE} samples max)")
            self.image_generated = True
        
        # Check buffer size before adding more samples to prevent overflow
        if self.speaker:
            current_buffer_size = self.speaker.buffer.size()
            
            # Only add samples if we have room (buffer not full)
            if current_buffer_size < MAX_SAMPLES_BUFFER_SIZE:
                samples = xos.random.uniform(low=AUDIO_MIN, high=AUDIO_MAX, shape=(TARGET_BATCH_SIZE,))
                self.speaker.play_samples(samples)
                
                # Print buffer status every 30 frames (~0.5 seconds at 60fps)
                if self.tick_count % 30 == 0:
                    buffer_percent = (current_buffer_size / MAX_SAMPLES_BUFFER_SIZE) * 100
                    xos.print(f"Buffer: {current_buffer_size}/{MAX_SAMPLES_BUFFER_SIZE} samples ({buffer_percent:.1f}%)")
            else:
                # Buffer is full - skip adding samples this frame
                if self.tick_count % 30 == 0:
                    xos.print(f"⚠️  Buffer full ({current_buffer_size} samples) - waiting for playback to catch up")


# Demo code to show how it would be used
if __name__ == "__main__":
    xos.print("🎬 Static with Audio - TV Static Simulator")
    xos.print("Displays continuous random visual static with white noise audio")
    xos.print(f"🔊 Audio: {SAMPLE_RATE} Hz stereo white noise stream")
    
    app = StaticWithAudio()
    app.run()
