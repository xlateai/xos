import { WebView } from "react-native-webview";
import { View } from "react-native";
import { Platform } from "react-native";

export default function App() {
  if (Platform.OS === "web") {
    return (
      <iframe
        src="http://localhost:8080"
        style={{ flex: 1, width: "100%", height: "100%", border: "none" }}
      />
    );
  }

  return (
    <View style={{ flex: 1 }}>
      <WebView
        source={{ uri: "http://localhost:8080" }}
        originWhitelist={["*"]}
        javaScriptEnabled
        allowFileAccess
        allowsInlineMediaPlayback
        startInLoadingState
      />
    </View>
  );
}
