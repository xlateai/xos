#!/usr/bin/env python3
"""
Audio Relay - Headless Real-time Audio Passthrough

Captures audio from the microphone and immediately plays it back through speakers.
This is a headless script (no viewport) that demonstrates real-time audio I/O.
Press Ctrl+C to stop.
"""

import xos
import time

# Configuration
SAMPLE_RATE = 44100  # 44.1 kHz
BUFFER_DURATION = 0.1  # 100ms microphone buffer to prevent overflow
BATCH_SIZE = 8192  # Large batch size to ensure we get ALL available samples
CHANNELS = 1  # Mono audio
GAIN = 3.0  # Amplify audio (3x volume boost)

def main():
    """Main audio relay loop"""
    xos.print("=" * 60)
    xos.print("🎤 → 🔊  Audio Relay - Real-time Passthrough")
    xos.print("=" * 60)
    xos.print("")
    
    # Get system type for device selection
    system_type = xos.system.get_system_type()
    
    # === Setup Microphone ===
    xos.print("🎤 Setting up microphone...")
    input_devices = xos.audio.get_input_devices()
    
    if not input_devices:
        xos.print("❌ No microphone devices found!")
        return
    
    if system_type == "IOS":
        mic_device_id = 0
        xos.print(f"   Using: {input_devices[0]['name']}")
    else:
        xos.print("\nAvailable microphones:")
        for i, dev in enumerate(input_devices):
            xos.print(f"  {i}: {dev['name']}")
        
        mic_device_id = xos.dialoguer.select(
            "Select microphone",
            [dev['name'] for dev in input_devices],
            default=0
        )
        xos.print(f"   Selected: {input_devices[mic_device_id]['name']}")
    
    # === Setup Speaker ===
    xos.print("\n🔊 Setting up speaker...")
    output_devices = xos.audio.get_output_devices()
    
    if not output_devices:
        xos.print("❌ No speaker devices found!")
        return
    
    if system_type == "IOS":
        speaker_device_id = 0
        xos.print(f"   Using: Built-in Speaker")
    else:
        xos.print("\nAvailable speakers:")
        for i, dev in enumerate(output_devices):
            xos.print(f"  {i}: {dev['name']}")
        
        speaker_device_id = xos.dialoguer.select(
            "Select speaker",
            [dev['name'] for dev in output_devices],
            default=0
        )
        xos.print(f"   Selected: {output_devices[speaker_device_id]['name']}")
    
    # === Initialize Audio Devices ===
    xos.print("\n⚙️  Initializing audio devices...")
    
    try:
        microphone = xos.audio.Microphone(
            device_id=mic_device_id,
            buffer_duration=BUFFER_DURATION
        )
        xos.print("   ✅ Microphone ready")
    except Exception as e:
        xos.print(f"   ❌ Failed to initialize microphone: {e}")
        return
    
    try:
        speaker = xos.audio.Speaker(
            device_id=speaker_device_id,
            sample_rate=SAMPLE_RATE,
            channels=CHANNELS
        )
        xos.print("   ✅ Speaker ready")
    except Exception as e:
        xos.print(f"   ❌ Failed to initialize speaker: {e}")
        return
    
    # === Start Relay ===
    xos.print("\n" + "=" * 60)
    xos.print("🎙️  LIVE - Audio relay active!")
    xos.print("   Speak into the microphone to hear yourself through speakers")
    xos.print("   Press Ctrl+C to stop")
    xos.print("=" * 60)
    xos.print("")
    
    # Statistics
    total_samples = 0
    start_time = time.time()
    batch_count = 0
    last_status_time = start_time
    
    # Debug: Check initial speaker buffer
    xos.print("🔍 Initial speaker buffer size:", speaker.samples_buffer.shape[0] if hasattr(speaker.samples_buffer, 'shape') else 0)
    
    try:
        # No sleep - process as fast as possible for lowest latency
        while True:
            # Read (drain) audio samples from microphone - prevents repeats and delay
            audio_batch = microphone.read(BATCH_SIZE)

            # this can be helpful actually as it will display the
            # stats for the array samples distribution nicely.
            # print(audio_batch)
            
            if audio_batch and audio_batch['_data']:
                samples = audio_batch['_data']
                sample_count = len(samples)
                
                # Only relay if we got samples
                if sample_count > 0:
                    # Debug: Log first few batches
                    if batch_count < 3:
                        xos.print(f"🔍 Batch {batch_count}: Got {sample_count} samples from mic")
                    
                    # Amplify samples for louder output
                    amplified_samples = [min(1.0, max(-1.0, s * GAIN)) for s in samples]
                    
                    # Immediately relay to speaker
                    speaker.play_sample_batch(amplified_samples)
                    
                    # Update statistics
                    total_samples += sample_count
                    batch_count += 1
            else:
                # No samples available - sleep for 1ms to avoid burning CPU
                time.sleep(0.001)
                
                # Print status every 2 seconds
                current_time = time.time()
                if current_time - last_status_time >= 2.0:
                    elapsed = current_time - start_time
                    samples_per_sec = total_samples / elapsed if elapsed > 0 else 0
                    
                    # Get speaker buffer status
                    try:
                        buffer_size = speaker.samples_buffer.shape[0]
                    except:
                        buffer_size = 0
                    
                    xos.print(f"📊 Status: {batch_count} batches, "
                             f"{total_samples:,} samples relayed, "
                             f"{samples_per_sec:,.0f} samples/sec, "
                             f"buffer: {buffer_size}")
                    
                    last_status_time = current_time
    
    except KeyboardInterrupt:
        xos.print("\n")
        xos.print("=" * 60)
        xos.print("🛑 Audio relay stopped by user")
        
        # Final statistics
        elapsed = time.time() - start_time
        xos.print(f"\n📈 Session Statistics:")
        xos.print(f"   Duration: {elapsed:.1f} seconds")
        xos.print(f"   Total batches: {batch_count:,}")
        xos.print(f"   Total samples: {total_samples:,}")
        xos.print(f"   Average rate: {total_samples/elapsed:,.0f} samples/sec")
        xos.print(f"   Expected rate: {SAMPLE_RATE:,} samples/sec")
        xos.print("=" * 60)
    
    except Exception as e:
        xos.print(f"\n❌ Error in audio relay: {e}")
        import traceback
        traceback.print_exc()
    
    finally:
        xos.print("\n🔌 Cleaning up audio devices...")
        xos.audio.cleanup_all_microphones()
        xos.audio.cleanup_all_speakers()
        xos.print("✨ Done!")

if __name__ == "__main__":
    main()

