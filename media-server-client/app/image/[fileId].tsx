import { Image, View } from "react-native";

import { useLocalSearchParams } from "expo-router";
import { API_HOST } from "@/constants/Config";

export default function HomeScreen() {
  const { fileId } = useLocalSearchParams();

  return (
    <View style={{ flexDirection: "column", flex: 1 }}>
      <Image
        source={{ uri: `${API_HOST}/media/image?id=${fileId}` }}
        style={{ flex: 1 }}
        resizeMode="contain"
      />
    </View>
  );
}
