#!/usr/bin/env python3
"""
UI Demo - Demonstrates xos.rasterizer.text() and xos.ui.button()

This demo shows:
- Text rendering with xos.rasterizer.text()
- Button drawing with xos.ui.button()
- Button interaction with xos.ui.button_contains()
"""

import xos


class UIDemo(xos.Application):
    def __init__(self):
        super().__init__()
        self.button_clicked = False
        self.click_count = 0
        
    def setup(self):
        """Initialize the demo"""
        xos.print("🎨 UI Demo - Starting...")
        xos.print("Click the button to see interactions!")
    
    def tick(self):
        """Update and render one frame"""
        # Clear background to dark gray
        xos.rasterizer.fill(self.frame, (30, 30, 30, 255))
        
        width = self.get_width()
        height = self.get_height()
        
        # Draw title text at the top
        xos.rasterizer.text(
            "XOS UI Demo",
            20.0, 20.0,  # x, y position
            32.0,  # font size
            (255, 255, 255),  # white color
            float(width - 40)  # max width (with padding)
        )
        
        # Draw subtitle
        xos.rasterizer.text(
            "Demonstrating text rendering and button components",
            20.0, 60.0,
            16.0,
            (200, 200, 200),
            float(width - 40)
        )
        
        # Draw a button in the center
        button_width = 200
        button_height = 60
        button_x = int((width - button_width) / 2)
        button_y = int((height - button_height) / 2)
        
        # Check if mouse is hovering over button
        mouse_x = self.mouse['x']
        mouse_y = self.mouse['y']
        is_hovered = xos.ui.button_contains(
            button_x, button_y, button_width, button_height,
            mouse_x, mouse_y
        )
        
        # Draw the button
        xos.ui.button(
            button_x, button_y, button_width, button_height,
            "Click Me!",  # text (not rendered yet)
            is_hovered,
            (50, 150, 200),  # bg_color (blue)
            (70, 170, 220),  # hover_color (lighter blue)
            (255, 255, 255)  # text_color (white)
        )
        
        # Draw button text manually using text rasterizer
        button_text = "Click Me!"
        text_size = 20
        # Center text in button (approximate)
        text_x = float(button_x + (button_width - len(button_text) * text_size * 0.5) / 2)
        text_y = float(button_y + (button_height - text_size) / 2)
        xos.rasterizer.text(
            button_text,
            text_x, text_y,
            float(text_size),
            (255, 255, 255),
            float(button_width - 20)
        )
        
        # Draw click counter at the bottom
        counter_text = f"Button clicked {self.click_count} times"
        xos.rasterizer.text(
            counter_text,
            20.0, float(height - 60),
            18.0,
            (100, 255, 100) if self.click_count > 0 else (150, 150, 150),
            float(width - 40)
        )
        
        # Draw some example text with different sizes
        example_y = height - 200
        xos.rasterizer.text("Small text (12px)", 20.0, float(example_y), 12.0, (255, 200, 100))
        xos.rasterizer.text("Medium text (16px)", 20.0, float(example_y + 20), 16.0, (255, 200, 100))
        xos.rasterizer.text("Large text (24px)", 20.0, float(example_y + 45), 24.0, (255, 200, 100))
        
        # Draw text with transparency
        xos.rasterizer.text(
            "Semi-transparent text",
            20.0, float(example_y + 80),
            16.0,
            (255, 255, 255, 128),  # 50% transparent white
            float(width - 40)
        )
    
    def on_mouse_down(self, x, y):
        """Handle mouse down event"""
        # Check if button was clicked
        width = self.get_width()
        height = self.get_height()
        
        button_width = 200
        button_height = 60
        button_x = int((width - button_width) / 2)
        button_y = int((height - button_height) / 2)
        
        if xos.ui.button_contains(button_x, button_y, button_width, button_height, x, y):
            self.click_count += 1
            xos.print(f"🖱️  Button clicked! Count: {self.click_count}")


if __name__ == "__main__":
    xos.print("🎨 UI Demo - Text Rendering & Buttons")
    xos.print("Demonstrating xos.rasterizer.text() and xos.ui.button()")
    
    app = UIDemo()
    app.run()

