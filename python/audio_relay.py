#!/usr/bin/env python3
"""
Audio Relay - Interactive Real-time Audio Passthrough with Toggle Button

Captures audio from the microphone and plays it back through speakers.
Click the centered square button to toggle audio relay on/off.
"""

import xos

# Configuration
SAMPLE_RATE = 44100  # 44.1 kHz
BUFFER_DURATION = 0.1  # 100ms microphone buffer to prevent overflow
BATCH_SIZE = 8192  # Large batch size to ensure we get ALL available samples
CHANNELS = 1  # Mono audio
GAIN = 3.0  # Amplify audio (3x volume boost)

# Toggle button configuration
BUTTON_SIZE = 60.0
BUTTON_BORDER_WIDTH = 3.0


class AudioRelay(xos.Application):
    def __init__(self):
        super().__init__()
        self.microphone = None
        self.speaker = None
        self.initialized = False
        self.last_buffer_size = 0
        self.enabled = False  # Audio processing on/off
        
    def setup(self):
        """Initialize audio devices"""
        xos.print("🎤 AudioRelay - Initializing...")
        
        # Get system type for device selection
        system_type = xos.system.get_system_type()
        
        # Get audio devices
        input_devices = xos.audio.get_input_devices()
        output_devices = xos.audio.get_output_devices()
        
        if not input_devices:
            xos.print("❌ No input devices found")
            return
        
        if not output_devices:
            xos.print("❌ No output devices found")
            return
        
        # Select input device
        if system_type == "IOS":
            mic_device_id = 0
        else:
            xos.print("\nAvailable microphones:")
            for i, dev in enumerate(input_devices):
                xos.print(f"  {i}: {dev['name']}")
            
            mic_device_id = xos.dialoguer.select(
                "Select input device (microphone)",
                [dev['name'] for dev in input_devices],
                default=0
            )
        
        xos.print(f"📍 Input: {input_devices[mic_device_id]['name']}")
        
        # Select output device
        if system_type == "IOS":
            speaker_device_id = 0
        else:
            xos.print("\nAvailable speakers:")
            for i, dev in enumerate(output_devices):
                xos.print(f"  {i}: {dev['name']}")
            
            speaker_device_id = xos.dialoguer.select(
                "Select output device (speakers)",
                [dev['name'] for dev in output_devices],
                default=0
            )
        
        xos.print(f"🔊 Output: {output_devices[speaker_device_id]['name']}")
        
        # Create audio devices
        try:
            self.microphone = xos.audio.Microphone(
                device_id=mic_device_id,
                buffer_duration=BUFFER_DURATION
            )
            xos.print("✅ Microphone created")
            
            # Pause microphone immediately (mic light OFF by default)
            # Note: The Python bridge starts recording by default, so we need to clean up and recreate
            # For now, the mic will stay on until we toggle it off
            
        except Exception as e:
            xos.print(f"❌ Failed to create microphone: {e}")
            return
        
        try:
            self.speaker = xos.audio.Speaker(
                device_id=speaker_device_id,
                sample_rate=SAMPLE_RATE,
                channels=CHANNELS
            )
            xos.print("✅ Speaker created")
        except Exception as e:
            xos.print(f"❌ Failed to create speaker: {e}")
            return
        
        self.initialized = True
        xos.print("✅ Devices ready! Click the centered square to start audio relay.")
    
    def tick(self):
        """Update and render one frame"""
        # Clear background (pitch black)
        xos.rasterizer.clear()
        
        # Draw toggle button
        self.draw_button()
        
        # Relay audio if initialized AND enabled
        if self.initialized and self.enabled:
            if self.microphone and self.speaker:
                # Get samples from microphone
                audio_batch = self.microphone.get_batch(BATCH_SIZE)
                
                if audio_batch and audio_batch['_data']:
                    samples = audio_batch['_data']
                    
                    if len(samples) > 0:
                        # Amplify samples for louder output
                        amplified_samples = [min(1.0, max(-1.0, s * GAIN)) for s in samples]
                        
                        # Queue amplified samples for playback
                        try:
                            self.speaker.play_sample_batch(amplified_samples)
                        except Exception as e:
                            xos.print(f"⚠️  Playback error: {e}")
                        
                        # Log buffer size occasionally
                        buffer_size = self.speaker.samples_buffer.shape[0] if hasattr(self.speaker.samples_buffer, 'shape') else 0
                        if buffer_size != self.last_buffer_size and buffer_size % 1000 == 0:
                            xos.print(f"📊 Buffer: {buffer_size} samples")
                            self.last_buffer_size = buffer_size
    
    def draw_button(self):
        """Draw the toggle button in the center of the screen"""
        width = self.get_width()
        height = self.get_height()
        
        # Center the button at 0.5, 0.5
        button_x = int((width - BUTTON_SIZE) / 2.0)
        button_y = int((height - BUTTON_SIZE) / 2.0)
        
        # Determine color based on enabled state
        if self.enabled:
            color = (0, 255, 0, 255)  # Green when enabled
        else:
            color = (100, 100, 100, 255)  # Gray when disabled
        
        # Draw button using rasterizer
        # If enabled, draw filled rectangle; if disabled, draw border only
        if self.enabled:
            # Draw filled square
            xos.rasterizer.rects_filled(
                self.frame,
                button_x,
                button_y,
                button_x + int(BUTTON_SIZE),
                button_y + int(BUTTON_SIZE),
                color
            )
        else:
            # Draw border (4 rectangles: top, bottom, left, right)
            border = int(BUTTON_BORDER_WIDTH)
            
            # Top border
            xos.rasterizer.rects_filled(self.frame, button_x, button_y, button_x + int(BUTTON_SIZE), button_y + border, color)
            # Bottom border
            xos.rasterizer.rects_filled(self.frame, button_x, button_y + int(BUTTON_SIZE) - border, button_x + int(BUTTON_SIZE), button_y + int(BUTTON_SIZE), color)
            # Left border
            xos.rasterizer.rects_filled(self.frame, button_x, button_y + border, button_x + border, button_y + int(BUTTON_SIZE) - border, color)
            # Right border
            xos.rasterizer.rects_filled(self.frame, button_x + int(BUTTON_SIZE) - border, button_y + border, button_x + int(BUTTON_SIZE), button_y + int(BUTTON_SIZE) - border, color)
    
    def on_mouse_down(self, x, y):
        """Handle mouse down event"""
        if not self.initialized:
            return
        
        # Get button center position
        width = self.get_width()
        height = self.get_height()
        button_x = (width - BUTTON_SIZE) / 2.0
        button_y = (height - BUTTON_SIZE) / 2.0
        
        # Check if click is inside button
        if (x >= button_x and x <= button_x + BUTTON_SIZE and
            y >= button_y and y <= button_y + BUTTON_SIZE):
            # Toggle enabled state
            self.enabled = not self.enabled
            
            if self.enabled:
                xos.print("🟢 Audio relay ENABLED")
            else:
                xos.print("⬜ Audio relay DISABLED")


if __name__ == "__main__":
    xos.print("🎤 → 🔊  Audio Relay")
    xos.print("Click the centered square button to toggle audio on/off")
    
    app = AudioRelay()
    app.run()
