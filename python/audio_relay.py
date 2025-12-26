#!/usr/bin/env python3
"""
Audio Relay - Interactive Real-time Audio Passthrough with Toggle Button

Captures audio from the microphone and plays it back through speakers.
Click the centered square button to toggle audio relay on/off.
"""

import xos

# Configuration
BUFFER_DURATION = 0.1  # 100ms microphone buffer to prevent overflow
BATCH_SIZE = 8192  # Large batch size to ensure we get ALL available samples
CHANNELS = 1  # Mono audio
GAIN = 3.0  # Amplify audio (3x volume boost)

# Toggle button configuration (will be calculated based on screen size)
BUTTON_SIZE_RATIO = 0.12  # 12% of smaller screen dimension
BUTTON_BORDER_WIDTH = 3.0


class AudioRelay(xos.Application):
    def __init__(self):
        super().__init__()
        self.microphone = None
        self.speaker = None
        self.last_buffer_size = 0
        self.enabled = False  # Audio processing on/off
        # Store device IDs (None = default device with auto-switching)
        self.mic_device_id = None
        self.speaker_device_id = None
        
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
            self.mic_device_id = None  # Use default on iOS
            xos.print(f"📍 Input: Default (auto-switching)")
        else:
            xos.print("\nAvailable microphones:")
            xos.print("  0: Default (auto-switching)")
            for i, dev in enumerate(input_devices):
                xos.print(f"  {i+1}: {dev['name']}")
            
            selection = xos.dialoguer.select(
                "Select input device (microphone)",
                ["Default (auto-switching)"] + [dev['name'] for dev in input_devices],
                default=0
            )
            
            self.mic_device_id = None if selection == 0 else selection - 1
            device_name = "Default (auto-switching)" if selection == 0 else input_devices[selection - 1]['name']
            xos.print(f"📍 Input: {device_name}")
        
        # Select output device
        if system_type == "IOS":
            self.speaker_device_id = None  # Use default on iOS
            xos.print(f"🔊 Output: Default (auto-switching)")
        else:
            xos.print("\nAvailable speakers:")
            xos.print("  0: Default (auto-switching)")
            for i, dev in enumerate(output_devices):
                xos.print(f"  {i+1}: {dev['name']}")
            
            selection = xos.dialoguer.select(
                "Select output device (speakers)",
                ["Default (auto-switching)"] + [dev['name'] for dev in output_devices],
                default=0
            )
            
            self.speaker_device_id = None if selection == 0 else selection - 1
            device_name = "Default (auto-switching)" if selection == 0 else output_devices[selection - 1]['name']
            xos.print(f"🔊 Output: {device_name}")
        
        # Create audio devices during setup for instant first toggle
        try:
            self.microphone = xos.audio.Microphone(
                device_id=self.mic_device_id,
                buffer_duration=BUFFER_DURATION
            )
            # Microphone starts paused by default (mic light OFF)
            xos.print("✅ Microphone created (paused by default)")
        except Exception as e:
            xos.print(f"❌ Failed to create microphone: {e}")
            return
        
        # Get the microphone's actual sample rate and use it for the speaker
        # This prevents pitch/speed issues (e.g., AirPods mic at 16kHz)
        mic_sample_rate = self.microphone.get_sample_rate()
        xos.print(f"🎤 Microphone sample rate: {mic_sample_rate} Hz")
        
        try:
            self.speaker = xos.audio.Speaker(
                device_id=self.speaker_device_id,
                sample_rate=mic_sample_rate,  # Match mic sample rate!
                channels=CHANNELS
            )
            xos.print(f"✅ Speaker created ({mic_sample_rate} Hz)")
        except Exception as e:
            xos.print(f"❌ Failed to create speaker: {e}")
            return
        
        xos.print("✅ Devices ready! Click the centered square to start audio relay.")
    
    def tick(self):
        """Update and render one frame"""
        # Clear background (pitch black)
        xos.rasterizer.clear()
        
        # Draw toggle button
        self.draw_button()
        
        # Relay audio if enabled AND microphone exists
        if self.enabled and self.microphone and self.speaker:
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
        
        # Calculate responsive button size (12% of smaller dimension)
        button_size = int(min(width, height) * BUTTON_SIZE_RATIO)
        
        # Center the button at 0.5, 0.5
        button_x = int((width - button_size) / 2.0)
        button_y = int((height - button_size) / 2.0)
        
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
                button_x + button_size,
                button_y + button_size,
                color
            )
        else:
            # Draw border (4 rectangles: top, bottom, left, right)
            border = int(BUTTON_BORDER_WIDTH)
            
            # Top border
            xos.rasterizer.rects_filled(self.frame, button_x, button_y, button_x + button_size, button_y + border, color)
            # Bottom border
            xos.rasterizer.rects_filled(self.frame, button_x, button_y + button_size - border, button_x + button_size, button_y + button_size, color)
            # Left border
            xos.rasterizer.rects_filled(self.frame, button_x, button_y + border, button_x + border, button_y + button_size - border, color)
            # Right border
            xos.rasterizer.rects_filled(self.frame, button_x + button_size - border, button_y + border, button_x + button_size, button_y + button_size - border, color)
    
    def on_mouse_down(self, x, y):
        """Handle mouse down event"""
        # Get button center position
        width = self.get_width()
        height = self.get_height()
        button_size = int(min(width, height) * BUTTON_SIZE_RATIO)
        button_x = (width - button_size) / 2.0
        button_y = (height - button_size) / 2.0
        
        # Check if click is inside button
        if (x >= button_x and x <= button_x + button_size and
            y >= button_y and y <= button_y + button_size):
            # Toggle enabled state
            self.enabled = not self.enabled
            
            if self.enabled:
                # Resume microphone - INSTANT! (device already created)
                if self.microphone:
                    try:
                        self.microphone.record()
                        xos.print("🟢 Audio relay ENABLED - Mic light ON")
                    except Exception as e:
                        xos.print(f"❌ Failed to resume microphone: {e}")
                        self.enabled = False
            else:
                # Pause microphone - INSTANT mic light OFF
                if self.microphone:
                    try:
                        self.microphone.pause()
                    except Exception as e:
                        xos.print(f"❌ Failed to pause microphone: {e}")
                
                # Clear speaker buffer to stop any queued audio
                # Note: Python bridge doesn't expose clear() yet, so audio will drain naturally
                
                xos.print("⬜ Audio relay DISABLED - Mic light OFF")


if __name__ == "__main__":
    xos.print("🎤 → 🔊  Audio Relay")
    xos.print("Click the centered square button to toggle audio on/off")
    
    app = AudioRelay()
    app.run()
