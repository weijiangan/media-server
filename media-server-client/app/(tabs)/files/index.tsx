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
import { useEffect, useRef, useState } from "react";
import {
  Href,
  Link,
  Stack,
  useLocalSearchParams,
  useRouter,
} from "expo-router";
import { API_HOST } from "@/constants/Config";

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
  type: "file" | "directory";
  width: number;
}

// eslint-disable-next-line @typescript-eslint/no-unused-vars
const statuses = ["init", "loading", "success", "error"] as const;
type Status = (typeof statuses)[number];

export default function HomeScreen() {
  const [data, setData] = useState<{ files: MediaItem[] }>();
  const searchParams = useLocalSearchParams();
  const prevPathId = useRef(searchParams.pathId);
  const status = useRef<Status>("init");

  useEffect(() => {
    if (
      prevPathId.current === searchParams.pathId &&
      status.current !== "init"
    ) {
      return;
    }

    let url = new URL(`${API_HOST}/media`);
    if (searchParams.pathId) {
      url.searchParams.append("parent_id", searchParams.pathId as string);
    }

    let options = {
      method: "GET",
      headers: {
        Accept: "*/*",
        "Accept-Encoding": "gzip, deflate, br",
        Connection: "keep-alive",
      },
    };

    status.current = "loading";
    fetch(url, options)
      .then((res) => res.json())
      .then((json) => {
        setData(json);
        status.current = "success";
      })
      .catch((err) => {
        console.error("error:" + err);
        status.current = "error";
      });

    prevPathId.current = searchParams.pathId;
  });

  return (
    <ThemedView>
      <Stack.Screen
        options={{ title: (searchParams.pathId as string) ?? "Files" }}
      ></Stack.Screen>
      <FlatList
        numColumns={2}
        contentContainerStyle={{ gap: 1 }}
        columnWrapperStyle={{ gap: 1 }}
        data={data?.files?.filter(
          (item) =>
            item.mime_type?.startsWith("image/") ||
            item.mime_type?.startsWith("video/") ||
            item.mime_type === null
        )}
        renderItem={({ item }) => {
          let href: Href = "/files";
          if (item.mime_type?.startsWith("video/")) {
            href = { pathname: `/video/[fileId]`, params: { fileId: item.id } };
          } else if (item.mime_type?.startsWith("image/")) {
            href = { pathname: `/image/[fileId]`, params: { fileId: item.id } };
          } else if (item.type === "directory") {
            href = { pathname: `/files`, params: { pathId: 1 } };
          }
          // else {
          //   href = `/file/[fileId]`;
          // }
          return (
            <Link push href={href} asChild>
              <TouchableOpacity style={{ flex: 1 }}>
                {item.mime_type ? (
                  <ImageBackground
                    style={{ height: 200 }}
                    source={{
                      uri:
                        (item.thumb_path && `${API_HOST}${item.thumb_path}`) ||
                        `${API_HOST}/media/thumbnail?id=${item.id}`,
                    }}
                  />
                ) : (
                  <ThemedView
                    style={{
                      flex: 1,
                      alignItems: "center",
                      justifyContent: "center",
                    }}
                  >
                    <ThemedText>{item.name}</ThemedText>
                  </ThemedView>
                )}
              </TouchableOpacity>
            </Link>
          );
        }}
      />
    </ThemedView>
  );
}

const styles = StyleSheet.create({});
