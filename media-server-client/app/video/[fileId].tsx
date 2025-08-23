import { StyleSheet, View } from "react-native";

import { useLocalSearchParams } from "expo-router";

const API_HOST = "http://192.168.0.12:8080";

export default function HomeScreen() {
  const { fileId } = useLocalSearchParams();

  return (
    <View>
      <video
        controls
        width="640"
        poster={`${API_HOST}/media/thumbnail?id=${fileId}`}
      >
        <source
          src={`${API_HOST}/media/stream?id=${fileId}`}
          type="video/mp4"
        />
        Your browser does not support the video tag.
      </video>
    </View>
  );
}

const styles = StyleSheet.create({});
