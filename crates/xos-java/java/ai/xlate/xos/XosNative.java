package ai.xlate.xos;

import java.nio.ByteBuffer;

/**
 * JNI bridge to the xos engine (xos-java crate). Native library name: {@code xos_java}
 * ({@code xos_java.dll} / {@code libxos_java.so} / {@code libxos_java.dylib}).
 * <p>
 * Call {@link #initLibrary(String)} once before any other method, with the absolute path to the
 * native library, <em>or</em> ensure {@code xos_java} is on {@code java.library.path} and use
 * {@link System#loadLibrary(String)} yourself before touching this class.
 * <p>
 * After {@link #resize}, any previously obtained {@link #getFrameBuffer()} may be invalid; obtain
 * a fresh direct buffer.
 */
public final class XosNative {

    private static volatile boolean loaded;

    private XosNative() {}

    /**
     * Loads {@code xos_java} from an absolute filesystem path (recommended for modded games).
     */
    public static synchronized void initLibrary(String absolutePathToNativeLibrary) {
        if (loaded) {
            return;
        }
        System.load(absolutePathToNativeLibrary);
        loaded = true;
    }

    /**
     * Loads {@code xos_java} from {@code java.library.path} (e.g. {@code -Djava.library.path=...}).
     */
    public static synchronized void initLibraryFromPath() {
        if (loaded) {
            return;
        }
        System.loadLibrary("xos_java");
        loaded = true;
    }

    /**
     * Low-level JNI smoke test: returns a string allocated on the Rust side.
     * Call after {@link #initLibraryFromPath()} or {@link #initLibrary(String)}.
     */
    public static native String ping();

    /**
     * Hello-world check for mods: same as {@link #ping()} — verifies xos_java is loaded and JNI works.
     */
    public static String helloWorld() {
        return ping();
    }

    public static native void init(int width, int height);

    public static native void shutdown();

    public static native void tick();

    /**
     * Number of completed native {@link #tick()} calls since {@link #init} (wraps on overflow).
     * Use this in the mod UI to confirm xos advances in lockstep with {@link #pumpFrame}.
     */
    public static native long getEngineTickCount();

    /**
     * Uniform alpha for the viewport texture when {@link #getFrameBuffer()} is packed for
     * {@link com.mojang.blaze3d.platform.NativeImage#setPixelRGBA}. Call before {@link #tick}
     * (e.g. each frame from the mod when idle vs hover changes).
     */
    public static native void setMinecraftViewportAlpha(int alpha0to255);

    /**
     * Direct buffer of packed pixels for Minecraft upload (little-endian int per pixel, same as
     * {@code setPixelRGBA} / Minekov {@code packAbgr}). Produced after {@link #tick}; avoids
     * per-pixel Java conversion from engine RGBA. Not valid after {@link #shutdown} or
     * {@link #resize} — obtain a fresh buffer.
     */
    public static native ByteBuffer getFrameBuffer();

    public static native void resize(int width, int height);

    public static native void onMouseMove(float x, float y);

    /** Button: 0 = left, 1 = right (matches common Minecraft / LWJGL conventions). */
    public static native void onMouseDown(int button);

    public static native void onMouseUp(int button);

    public static native void onScroll(float dx, float dy);

    /** Unicode code point (e.g. from {@link Character#codePointAt(CharSequence, int)}). */
    public static native void onKeyChar(int codepoint);

    /**
     * F3 toggles the global FPS overlay (same as desktop/winit). Not sent as a Unicode character.
     */
    public static native void onF3();
}
