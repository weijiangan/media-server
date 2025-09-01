import { StyleSheet, View } from "react-native";

import { useLocalSearchParams } from "expo-router";
import { API_HOST } from "@/constants/Config";

export default function HomeScreen() {
  const { fileId } = useLocalSearchParams();

  return (
    <View
      style={{ flexDirection: "column", flex: 1, justifyContent: "center" }}
    >
      <video controls poster={`${API_HOST}/media/thumbnail?id=${fileId}`}>
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
