#!/usr/bin/env python3
"""
Audio Relay with Device Selection Menu - Interactive Real-time Audio Passthrough

Captures audio from the microphone and plays it back through speakers.
- Quick tap: Toggle audio relay on/off
- Hold for 1 second: Open device selection menu to switch input/output devices
"""

import xos
import time

# Configuration
BUFFER_DURATION = 0.1  # 100ms microphone buffer (prevents overflow at 60fps = 16.67ms/frame)
CHANNELS = 1  # Mono audio
GAIN = 3.0  # Amplify audio (3x volume boost)

# Toggle button configuration
BUTTON_SIZE_RATIO = 0.12  # 12% of smaller screen dimension
BUTTON_BORDER_WIDTH = 3.0

# Menu configuration
HOLD_DURATION = 1.0  # 1 second hold to open menu
MENU_PADDING = 20
MENU_ITEM_HEIGHT = 40
MENU_COLUMN_WIDTH_RATIO = 0.4  # 40% of screen width per column


class AudioRelayWithMenu(xos.Application):
    def __init__(self):
        super().__init__()
        self.microphone = None
        self.speaker = None
        self.last_buffer_size = 0
        self.enabled = False  # Audio processing on/off
        
        # Device selection
        self.input_devices = []
        self.output_devices = []
        self.selected_input_index = -1  # -1 means "Default"
        self.selected_output_index = -1  # -1 means "Default"
        
        # Menu state
        self.show_menu = False
        self.mouse_down_time = None
        
    def setup(self):
        """Initialize audio devices"""
        xos.print("🎤 AudioRelay with Menu - Initializing...")
        
        # Get audio devices
        self.input_devices = xos.audio.get_input_devices()
        self.output_devices = xos.audio.get_output_devices()
        
        if not self.input_devices:
            xos.print("❌ No input devices found")
            return
        
        if not self.output_devices:
            xos.print("❌ No output devices found")
            return
        
        xos.print(f"📱 Found {len(self.input_devices)} input devices")
        xos.print(f"🔊 Found {len(self.output_devices)} output devices")
        
        # Create audio devices with default settings
        self.recreate_audio_devices()
        
        xos.print("✅ Devices ready!")
        xos.print("   - Quick tap: Toggle audio on/off")
        xos.print("   - Hold 1s: Open device selection menu")
    
    def recreate_audio_devices(self):
        """Recreate audio devices with current selection"""
        was_enabled = self.enabled
        self.enabled = False
        
        # Cleanup old devices
        if self.microphone:
            try:
                self.microphone.pause()
            except:
                pass
            self.microphone = None
        
        if self.speaker:
            self.speaker = None
        
        # Determine device IDs
        mic_device_id = None if self.selected_input_index == -1 else self.selected_input_index
        speaker_device_id = None if self.selected_output_index == -1 else self.selected_output_index
        
        # Create microphone
        try:
            self.microphone = xos.audio.Microphone(
                device_id=mic_device_id,
                buffer_duration=BUFFER_DURATION
            )
            device_name = "Default" if mic_device_id is None else self.input_devices[mic_device_id]['name']
            xos.print(f"🎤 Input: {device_name}")
        except Exception as e:
            xos.print(f"❌ Failed to create microphone: {e}")
            return
        
        # Get microphone sample rate
        mic_sample_rate = self.microphone.get_sample_rate()
        
        # Create speaker
        try:
            self.speaker = xos.audio.Speaker(
                device_id=speaker_device_id,
                sample_rate=mic_sample_rate,
                channels=CHANNELS
            )
            device_name = "Default" if speaker_device_id is None else self.output_devices[speaker_device_id]['name']
            xos.print(f"🔊 Output: {device_name}")
        except Exception as e:
            xos.print(f"❌ Failed to create speaker: {e}")
            return
        
        # Restore enabled state
        self.enabled = was_enabled
        if self.enabled and self.microphone:
            try:
                self.microphone.record()
            except:
                pass
    
    def tick(self):
        """Update and render one frame"""
        # Clear background (pitch black)
        xos.rasterizer.clear()
        
        # Check if menu should be shown (hold timer)
        if self.mouse_down_time is not None:
            elapsed = time.time() - self.mouse_down_time
            if elapsed >= HOLD_DURATION and not self.show_menu:
                self.show_menu = True
                xos.print("📱 Opening device selection menu...")
        
        # Draw UI
        if self.show_menu:
            self.draw_menu()
        else:
            self.draw_button()
        
        # Relay audio if enabled AND not in menu
        if self.enabled and not self.show_menu and self.microphone and self.speaker:
            audio_batch = self.microphone.read()
            self.speaker.play_samples(audio_batch, gain=GAIN)
    
    def draw_button(self):
        """Draw the toggle button in the center of the screen"""
        width = self.get_width()
        height = self.get_height()
        
        # Calculate responsive button size
        button_size = int(min(width, height) * BUTTON_SIZE_RATIO)
        button_x = int((width - button_size) / 2.0)
        button_y = int((height - button_size) / 2.0)
        
        # Use the new xos.ui.button API
        color = (0, 255, 0) if self.enabled else (100, 100, 100)
        hover_color = (0, 200, 0) if self.enabled else (120, 120, 120)
        
        xos.ui.button(
            button_x, button_y, button_size, button_size,
            "Toggle",  # Text (not rendered yet)
            False,  # is_hovered
            color,  # bg_color
            hover_color,  # hover_color
            (255, 255, 255)  # text_color
        )
    
    def draw_menu(self):
        """Draw the device selection menu"""
        width = self.get_width()
        height = self.get_height()
        
        # Calculate menu dimensions
        column_width = int(width * MENU_COLUMN_WIDTH_RATIO)
        gap = 20
        left_column_x = int((width - column_width * 2 - gap) / 2)
        right_column_x = left_column_x + column_width + gap
        menu_y = MENU_PADDING
        
        # Draw left column (Input devices)
        self.draw_device_column(
            left_column_x, menu_y, column_width,
            "Input", self.input_devices, self.selected_input_index
        )
        
        # Draw right column (Output devices)
        self.draw_device_column(
            right_column_x, menu_y, column_width,
            "Output", self.output_devices, self.selected_output_index
        )
    
    def draw_device_column(self, x, y, column_width, title, devices, selected_index):
        """Draw a device selection column"""
        item_height = MENU_ITEM_HEIGHT
        
        # Draw title
        xos.rasterizer.rects_filled(
            self.frame, x, y, x + column_width, y + item_height,
            (60, 60, 60, 255)
        )
        xos.rasterizer.text(title, x + 10, y + 10, 20, (255, 255, 255), column_width - 20)
        
        # Draw "Default" option
        default_y = y + item_height + 5
        default_color = (0, 120, 255, 255) if selected_index == -1 else (80, 80, 80, 255)
        xos.rasterizer.rects_filled(
            self.frame, x, default_y, x + column_width, default_y + item_height,
            default_color
        )
        xos.rasterizer.text("Default", x + 10, default_y + 10, 16, (255, 255, 255), column_width - 20)
        
        # Draw device options
        for i, device in enumerate(devices):
            item_y = default_y + item_height + 5 + i * (item_height + 5)
            if item_y + item_height >= self.get_height():
                break
            
            item_color = (0, 120, 255, 255) if i == selected_index else (80, 80, 80, 255)
            xos.rasterizer.rects_filled(
                self.frame, x, item_y, x + column_width, item_y + item_height,
                item_color
            )
            
            # Truncate device name if too long
            device_name = device['name']
            if len(device_name) > 30:
                device_name = device_name[:27] + "..."
            
            xos.rasterizer.text(device_name, x + 10, item_y + 10, 14, (255, 255, 255), column_width - 20)
    
    def on_mouse_down(self, x, y):
        """Handle mouse down event"""
        # Start hold timer
        self.mouse_down_time = time.time()
        
        # Handle menu interactions if menu is shown
        if self.show_menu:
            self.handle_menu_click(x, y)
    
    def on_mouse_up(self, x, y):
        """Handle mouse up event"""
        # Get hold duration
        hold_duration = 0
        if self.mouse_down_time is not None:
            hold_duration = time.time() - self.mouse_down_time
        
        # Clear hold timer
        self.mouse_down_time = None
        
        # If menu is showing, just close it
        if self.show_menu:
            self.show_menu = False
            return
        
        # If it was a quick tap (not a hold), toggle audio
        if hold_duration < HOLD_DURATION:
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
                    if self.microphone:
                        try:
                            self.microphone.record()
                            xos.print("🟢 Audio relay ENABLED - Mic light ON")
                        except Exception as e:
                            xos.print(f"❌ Failed to resume microphone: {e}")
                            self.enabled = False
                else:
                    if self.microphone:
                        try:
                            self.microphone.pause()
                        except Exception as e:
                            xos.print(f"❌ Failed to pause microphone: {e}")
                    xos.print("⬜ Audio relay DISABLED - Mic light OFF")
    
    def handle_menu_click(self, mouse_x, mouse_y):
        """Handle clicks in the device selection menu"""
        width = self.get_width()
        
        # Calculate menu dimensions
        column_width = int(width * MENU_COLUMN_WIDTH_RATIO)
        gap = 20
        left_column_x = int((width - column_width * 2 - gap) / 2)
        right_column_x = left_column_x + column_width + gap
        menu_y = MENU_PADDING
        item_height = MENU_ITEM_HEIGHT
        
        # Check input column
        if mouse_x >= left_column_x and mouse_x < left_column_x + column_width:
            self.handle_column_click(mouse_y, menu_y, item_height, self.input_devices, True)
        
        # Check output column
        if mouse_x >= right_column_x and mouse_x < right_column_x + column_width:
            self.handle_column_click(mouse_y, menu_y, item_height, self.output_devices, False)
    
    def handle_column_click(self, mouse_y, menu_y, item_height, devices, is_input):
        """Handle click in a device column"""
        default_y = menu_y + item_height + 5
        
        # Check if clicked on "Default"
        if mouse_y >= default_y and mouse_y < default_y + item_height:
            if is_input:
                self.selected_input_index = -1
                xos.print("🔄 Switched to default input device")
            else:
                self.selected_output_index = -1
                xos.print("🔄 Switched to default output device")
            self.recreate_audio_devices()
            return
        
        # Check device list
        first_device_y = default_y + item_height + 5
        if mouse_y >= first_device_y:
            device_index = int((mouse_y - first_device_y) / (item_height + 5))
            if device_index < len(devices):
                if is_input:
                    self.selected_input_index = device_index
                    xos.print(f"🔄 Switched to input: {devices[device_index]['name']}")
                else:
                    self.selected_output_index = device_index
                    xos.print(f"🔄 Switched to output: {devices[device_index]['name']}")
                self.recreate_audio_devices()


if __name__ == "__main__":
    xos.print("🎤 → 🔊  Audio Relay with Device Selection")
    xos.print("Quick tap: Toggle audio on/off")
    xos.print("Hold 1s: Open device menu")
    
    app = AudioRelayWithMenu()
    app.run()

