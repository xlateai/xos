import { WebView } from "react-native-webview";
import { View } from "react-native";
import { Platform } from "react-native";

export default function App() {
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
        javaScriptEnabled
        allowFileAccess
        allowsInlineMediaPlayback
        startInLoadingState
      />
    </View>
  );
}
