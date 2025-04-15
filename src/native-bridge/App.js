import { WebView } from "react-native-webview";
import { View } from "react-native";
import { Platform } from "react-native";
import { useState, useEffect } from "react";

export default function App() {
  const [allowWebViewScroll, setAllowWebViewScroll] = useState(false)

  // it's truly tragic that this is required to make scrolling actually disable.
  useEffect(() => {
    // FIXME: temporal logic - https://github.com/react-native-webview/react-native-webview/issues/3608
    setAllowWebViewScroll(true)
    setTimeout(() => {
      setAllowWebViewScroll(false)
    }, 300)
  }, [])

  if (Platform.OS === "web") {
    return (
      <iframe
        src="https://solid-spork-4q6v7564xx53qgqw-8080.app.github.dev/"
        style={{ flex: 1, width: "100%", height: "100%", border: "none" }}
      />
    );
  }

  return (
    <View style={{ flex: 1 }}>
      <WebView
        source={{ uri: "https://solid-spork-4q6v7564xx53qgqw-8080.app.github.dev/" }}
        originWhitelist={["*"]}
        scrollEnabled={allowWebViewScroll}
        javaScriptEnabled
        allowFileAccess
        allowsInlineMediaPlayback
        startInLoadingState
      />
    </View>
  );
}
