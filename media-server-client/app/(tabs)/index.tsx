import {
  FlatList,
  ImageBackground,
  Platform,
  StyleSheet,
  TouchableOpacity,
  View,
} from "react-native";

import { ThemedText } from "@/components/ThemedText";
import { ThemedView } from "@/components/ThemedView";
import { useEffect, useState } from "react";
import { Link } from "expo-router";

interface MediaItem {
  created_at: string;
  duration_secs: number;
  height: number;
  id: number;
  mime_type: string | null;
  name: string;
  parent_id: number | null;
  path: string;
  size: number;
  tags: string | null;
  thumb_path: string | null;
  width: number;
}

const API_HOST = "http://192.168.0.12:8080";

export default function HomeScreen() {
  const [data, setData] = useState<{ files: MediaItem[] }>();
  useEffect(() => {
    let url = `${API_HOST}/media`;

    let options = {
      method: "GET",
      headers: {
        Accept: "*/*",
        "Accept-Encoding": "gzip, deflate, br",
        Connection: "keep-alive",
      },
    };

    fetch(url, options)
      .then((res) => res.json())
      .then((json) => setData(json))
      .catch((err) => console.error("error:" + err));
  }, []);

  return (
    <FlatList
      numColumns={2}
      contentContainerStyle={{ gap: 2 }}
      columnWrapperStyle={{ gap: 2 }}
      data={data?.files?.filter(
        (item) =>
          item.mime_type?.startsWith("image/") ||
          item.mime_type?.startsWith("video/") ||
          item.mime_type === null
      )}
      renderItem={({ item }) => (
        <TouchableOpacity>
          <Link href={`/video/${item.id}`}>
            <View style={{ overflow: "hidden" }}>
              <ImageBackground
                style={{ width: 200, height: 200 }}
                source={{
                  uri:
                    (item.thumb_path && `${API_HOST}${item.thumb_path}`) ||
                    `${API_HOST}/media/thumbnail?id=${item.id}`,
                }}
              />
            </View>
          </Link>
        </TouchableOpacity>
      )}
    />
  );
}

const styles = StyleSheet.create({
  titleContainer: {
    flexDirection: "row",
    alignItems: "center",
    gap: 8,
  },
  stepContainer: {
    gap: 8,
    marginBottom: 8,
  },
  reactLogo: {
    height: 178,
    width: 290,
    bottom: 0,
    left: 0,
    position: "absolute",
  },
});
