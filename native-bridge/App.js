import { Platform } from "react-native";
import { WebView } from "react-native-webview";
import * as Asset from "expo-asset";
import { useEffect, useState } from "react";
import { View } from "react-native";

export default function App() {
  const [htmlUri, setHtmlUri] = useState(null);

  useEffect(() => {
    if (Platform.OS !== "web") {
      const asset = Asset.Asset.fromModule(require("./assets/web/index.html"));
      asset.downloadAsync().then(() => {
        setHtmlUri(asset.localUri);
      });
    }
  }, []);

  if (Platform.OS === "web") {
    return (
      <iframe
        src="/assets/web/index.html"
        style={{ flex: 1, width: "100%", height: "100%", border: "none" }}
      />
    );
  }

  return (
    <View style={{ flex: 1 }}>
      {htmlUri && (
        <WebView
          originWhitelist={["*"]}
          source={{ uri: htmlUri }}
          allowFileAccess
          javaScriptEnabled
          allowsInlineMediaPlayback
        />
      )}
    </View>
  );
}
