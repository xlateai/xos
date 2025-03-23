import { WebView } from 'react-native-webview';
import { Asset } from 'expo-asset';
import { Platform } from 'react-native';

const assetUri = Asset.fromModule(require('./assets/web/index.html')).uri;

export default function App() {
  return (
    <WebView
      originWhitelist={['*']}
      source={
        Platform.OS === 'web'
          ? { uri: '/assets/web/index.html' } // optional for Expo web
          : { uri: assetUri }
      }
      javaScriptEnabled
      allowsInlineMediaPlayback
    />
  );
}
