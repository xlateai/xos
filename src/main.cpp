#include <SFML/Graphics.hpp>
#include <chrono>
#include <thread>

const int WINDOW_SIZE = 256;
const int TARGET_FPS = 60;
const std::chrono::microseconds FRAME_DURATION(1000000 / TARGET_FPS);

int main() {
    // Create window
    sf::RenderWindow window(sf::VideoMode(WINDOW_SIZE, WINDOW_SIZE), "Simple Viewport");
    
    // Create image and texture
    sf::Image image;
    image.create(WINDOW_SIZE, WINDOW_SIZE, sf::Color::Black);
    
    sf::Texture texture;
    texture.create(WINDOW_SIZE, WINDOW_SIZE);
    
    sf::Sprite sprite;
    sprite.setTexture(texture);
    
    // Game loop
    while (window.isOpen()) {
        // Handle events
        sf::Event event;
        while (window.pollEvent(event)) {
            if (event.type == sf::Event::Closed)
                window.close();
        }
        
        // Start frame timing
        auto frameStart = std::chrono::high_resolution_clock::now();
        
        // Clear image to black
        image.create(WINDOW_SIZE, WINDOW_SIZE, sf::Color::Black);
        
        // Set pixel at (100, 100) to white
        image.setPixel(100, 100, sf::Color::White);
        
        // Update texture and draw
        texture.update(image);
        window.clear();
        window.draw(sprite);
        window.display();
        
        // Frame timing to maintain target FPS
        auto frameEnd = std::chrono::high_resolution_clock::now();
        auto elapsedTime = std::chrono::duration_cast<std::chrono::microseconds>(frameEnd - frameStart);
        
        if (elapsedTime < FRAME_DURATION) {
            std::this_thread::sleep_for(FRAME_DURATION - elapsedTime);
        }
    }
    
    return 0;
}